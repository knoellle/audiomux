use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use anyhow::Ok;
use jack::{AudioIn, AudioOut, Client, Port};

enum BufferItem {
    Samples(Vec<Vec<f32>>),
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

        let mut state = self.jack_state.lock().unwrap();
        state.output.push(output);
        state.inputs.push(Input::new(&client, "1", 1));

        drop(state);

        let jack_state = self.jack_state.clone();
        let process_callback =
            move |_client: &jack::Client, scope: &jack::ProcessScope| -> jack::Control {
                let mut state = jack_state.lock().unwrap();

                for input in state.inputs.iter_mut() {
                    let silent = input
                        .ports
                        .iter()
                        .all(|port| port.as_slice(scope).iter().all(|f| f.abs() < 0.01));
                    if silent {
                        continue;
                    }
                    let samples = input
                        .ports
                        .iter()
                        .map(|port| Vec::from(port.as_slice(scope)))
                        .collect();

                    input.buffer.push_back(BufferItem::Samples(samples));
                }

                let buffer_item = match state
                    .inputs
                    .iter_mut()
                    .find(|input| !input.buffer.is_empty())
                {
                    Some(input) => input.buffer.pop_front().unwrap(),
                    None => {
                        state
                            .output
                            .iter_mut()
                            .for_each(|port| port.as_mut_slice(scope).fill(0.0));
                        return jack::Control::Continue;
                    }
                };
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
                }
                jack::Control::Continue
            };
        let process = jack::ClosureProcessHandler::new(process_callback);
        let _active_client = client
            .activate_async((), process)
            .expect("Failed to activate client");

        loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    }
}

fn main() -> anyhow::Result<()> {
    let multiplexer = Multiplexer::new();
    multiplexer.run().unwrap();

    Ok(())
}
