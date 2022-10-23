use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use anyhow::Ok;
use jack::{AudioIn, AudioOut, Client, Port};

enum BufferItem {
    Samples(Vec<Vec<f32>>),
    Silence(usize),
}

#[derive(Default)]
struct Input {
    ports: Vec<Port<AudioIn>>,
    buffer: VecDeque<BufferItem>,
}

impl Input {
    fn new(client: &Client, prefix: &str, channel_count: usize) -> Self {
        let ports = (0..channel_count)
            .map(|index| {
                client
                    .register_port(
                        format!("{prefix}.{index}").as_str(),
                        jack::AudioIn::default(),
                    )
                    .expect("Failed to register port")
            })
            .collect();
        Self {
            ports,
            buffer: VecDeque::new(),
        }
    }

    fn buffered_samples(&self) -> usize {
        self.buffer
            .iter()
            .map(|item| match item {
                BufferItem::Samples(samples) => samples[0].len(),
                BufferItem::Silence(_) => 0,
            })
            .sum()
    }

    fn urgency(&self) -> f32 {
        let silence_penalty = match self.buffer.front() {
            Some(BufferItem::Silence(count)) => *count as f32,
            _ => 0.0,
        };
        (self.buffered_samples() as f32).sqrt() - silence_penalty
    }
}

#[derive(Default)]
struct JackState {
    inputs: Vec<Input>,
    output: Vec<Port<AudioOut>>,
}

struct Multiplexer {
    jack_state: Arc<Mutex<JackState>>,
}

impl Multiplexer {
    fn new() -> Self {
        let jack_state = Arc::new(Mutex::new(JackState::default()));

        Multiplexer { jack_state }
    }

    fn run(&self) -> anyhow::Result<()> {
        let (client, _status) =
            jack::Client::new("Audio Multiplexer", jack::ClientOptions::NO_START_SERVER)
                .expect("Failed to create jack client");

        let output = client
            .register_port("0", jack::AudioOut::default())
            .expect("Failed to register port");
        let output2 = client
            .register_port("1", jack::AudioOut::default())
            .expect("Failed to register port");

        let mut state = self.jack_state.lock().unwrap();
        state.output.push(output);
        state.output.push(output2);
        state.inputs.push(Input::new(&client, "1", 2));
        state.inputs.push(Input::new(&client, "2", 2));

        drop(state);

        let jack_state = self.jack_state.clone();
        let process_callback =
            move |_client: &jack::Client, scope: &jack::ProcessScope| -> jack::Control {
                let mut state = jack_state.lock().unwrap();

                let frame_size = state.inputs[0].ports[0].as_slice(scope).len();

                for input in state.inputs.iter_mut() {
                    let silent = input
                        .ports
                        .iter()
                        .all(|port| port.as_slice(scope).iter().all(|f| f.abs() < 0.01));
                    if silent {
                        match input.buffer.back_mut() {
                            // Last item is silence, increase duration
                            Some(BufferItem::Silence(samples_remaining)) => {
                                *samples_remaining = 4800.min(*samples_remaining + frame_size)
                            }
                            // Buffer empty? Keep it that way to prevent latency when something
                            // does come in
                            None => {}
                            // Samples are buffered, store silence to keep somewhat natural pacing
                            _ => input.buffer.push_back(BufferItem::Silence(frame_size)),
                        }

                        continue;
                    }
                    // Skip silence if new samples come in
                    if input.buffer.len() == 1
                        && matches!(input.buffer.back(), Some(BufferItem::Silence(_)))
                    {
                        input.buffer.pop_front();
                    }
                    let samples = input
                        .ports
                        .iter()
                        .map(|port| Vec::from(port.as_slice(scope)))
                        .collect();

                    input.buffer.push_back(BufferItem::Samples(samples));
                }

                let mut sorted_inputs: Vec<_> = state.inputs.iter_mut().collect();
                sorted_inputs.sort_by(|a, b| b.urgency().total_cmp(&a.urgency()));
                let input = match sorted_inputs
                    .iter_mut()
                    .find(|input| input.buffered_samples() > 0)
                {
                    Some(input) => input,
                    None => {
                        state
                            .output
                            .iter_mut()
                            .for_each(|port| port.as_mut_slice(scope).fill(0.0));
                        return jack::Control::Continue;
                    }
                };
                let buffer_item = input.buffer.pop_front().unwrap();
                match buffer_item {
                    BufferItem::Samples(samples) => {
                        state
                            .output
                            .iter_mut()
                            .zip(samples.iter())
                            .for_each(|(port, samples)| {
                                port.as_mut_slice(scope).clone_from_slice(samples)
                            });
                    }
                    BufferItem::Silence(sample_count) => {
                        let silence_remaining =
                            sample_count as isize - input.ports[0].as_slice(scope).len() as isize;
                        if silence_remaining > 0 {
                            input
                                .buffer
                                .push_front(BufferItem::Silence(silence_remaining as usize));
                        }
                        state
                            .output
                            .iter_mut()
                            .for_each(|port| port.as_mut_slice(scope).fill(0.0));
                    }
                }
                jack::Control::Continue
            };
        let process = jack::ClosureProcessHandler::new(process_callback);
        let _active_client = client
            .activate_async((), process)
            .expect("Failed to activate client");

        loop {
            {
                let state = self.jack_state.lock().unwrap();
                println!();
                for input in state.inputs.iter() {
                    print!("Input: [");
                    for item in input.buffer.iter() {
                        match item {
                            BufferItem::Samples(..) => {
                                print!("s")
                            }
                            BufferItem::Silence(..) => print!("_"),
                        }
                    }
                    println!("]");
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

fn main() -> anyhow::Result<()> {
    let multiplexer = Multiplexer::new();
    multiplexer.run().unwrap();

    Ok(())
}
