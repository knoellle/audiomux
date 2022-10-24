use std::{
    collections::VecDeque,
    os::raw::c_void,
    process::Command,
    sync::{Arc, Mutex},
};

use anyhow::Ok;
use interleave_all::interleave_all;
use jack::{AudioIn, AudioOut, Client, Control, Port, ProcessScope};
use sound_touch::SoundTouch;
use soundtouch_sys::soundtouch_SoundTouch;
mod interleave_all;
mod sound_touch;

enum BufferItem {
    Samples(Vec<Vec<f32>>),
    Silence(usize),
}

struct AutoPausing {
    source_paused: bool,
    pause_threshold: usize,
    resume_threshold: usize,
    pause_command: String,
    resume_command: String,
}

#[derive(Default)]
struct Input {
    ports: Vec<Port<AudioIn>>,
    buffer: VecDeque<BufferItem>,
    pausing: Option<AutoPausing>,
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
            pausing: None,
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
    soundtouch: SoundTouch,
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
            Client::new("Audio Multiplexer", jack::ClientOptions::NO_START_SERVER)
                .expect("Failed to create jack client");

        let mut state = self.jack_state.lock().unwrap();

        let channel_count = 2;
        state.soundtouch.set_channels(channel_count as u32);
        state
            .soundtouch
            .set_sample_rate(client.sample_rate() as u32);

        state.output.extend((0..channel_count).map(|index| {
            client
                .register_port(format!("{index}").as_str(), jack::AudioOut::default())
                .expect("Failed to register port")
        }));
        state.inputs.push(Input::new(&client, "1", channel_count));
        let mut second_input = Input::new(&client, "2", channel_count);
        second_input.pausing = Some(AutoPausing {
            source_paused: false,
            pause_threshold: 48000,
            resume_threshold: 4800,
            pause_command: "playerctl pause".to_string(),
            resume_command: "playerctl play".to_string(),
        });
        state.inputs.push(second_input);

        drop(state);

        let jack_state = self.jack_state.clone();
        let process_callback =
            move |_client: &Client, scope: &ProcessScope| -> Control {
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

                let mut written_samples = 0;
                while written_samples < frame_size {
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
                            return Control::Continue;
                        }
                    };

                    let buffer_item = input.buffer.pop_front().unwrap();
                    match buffer_item {
                        BufferItem::Samples(samples) => {
                            let mut mixed_samples: Vec<f32> = interleave_all(samples).collect();
                            let channels = state.output.len();

                            state
                                .soundtouch
                                .put_samples(&mixed_samples, mixed_samples.len());

                            let requested_sample_count = (frame_size - written_samples) * channels;
                            let num_samples = state
                                .soundtouch
                                .receive_samples(&mut mixed_samples, requested_sample_count);
                            println!("Requested: {}", requested_sample_count);
                            println!("Mixed: {}", mixed_samples.len());
                            mixed_samples.truncate(num_samples);
                            println!("Mixed: {}", mixed_samples.len());

                            let unmixed_samples = (0..channels).map(|index| {
                                let x = mixed_samples
                                    .iter()
                                    .skip(index)
                                    .step_by(channels)
                                    .cloned()
                                    .collect::<Vec<f32>>();
                                println!("Got: {}", x.len());
                                x
                            });
                            state.output.iter_mut().zip(unmixed_samples).for_each(
                                |(port, samples)| {
                                    port.as_mut_slice(scope)[written_samples..]
                                        .clone_from_slice(&samples)
                                },
                            );
                            written_samples += num_samples;
                        }
                        BufferItem::Silence(sample_count) => {
                            let silence_remaining = sample_count as isize
                                - input.ports[0].as_slice(scope).len() as isize;
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
                }
                Control::Continue
            };
        let process = jack::ClosureProcessHandler::new(process_callback);
        let _active_client = client
            .activate_async((), process)
            .expect("Failed to activate client");

        loop {
            {
                let mut state = self.jack_state.lock().unwrap();
                println!();
                for input in state.inputs.iter_mut() {
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
                    println!("{}", input.urgency());
                    let buffered_samples = input.buffered_samples();
                    if let Some(pausing) = input.pausing.as_mut() {
                        if pausing.source_paused && buffered_samples < pausing.resume_threshold {
                            Command::new("bash")
                                .arg("-c")
                                .arg(&pausing.resume_command)
                                .spawn()
                                .unwrap();
                            pausing.source_paused = false;
                        }
                        if !pausing.source_paused && buffered_samples > pausing.pause_threshold {
                            Command::new("bash")
                                .arg("-c")
                                .arg(&pausing.pause_command)
                                .spawn()
                                .unwrap();
                            pausing.source_paused = true;
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

fn main() -> anyhow::Result<()> {
    unsafe {
        let mut soundtouch = soundtouch_SoundTouch::new();
        soundtouch.setSampleRate(48000);
        soundtouch.setChannels(1);
        soundtouch.setTempo(2.0);
        // soundtouch.setSetting(sound_touch::SETTING_SEQUENCE_MS, 40);
        // soundtouch.setSetting(sound_touch::SETTING_SEEKWINDOW_MS, 15);
        // soundtouch.setSetting(sound_touch::SETTING_OVERLAP_MS, 8);
        let samples: Vec<f32> = (0..48000).map(|index| (index as f32).sin()).collect();
        soundtouch_sys::soundtouch_SoundTouch_putSamples(
            &mut soundtouch as *mut _ as *mut c_void,
            samples.as_ptr(),
            samples.len() as u32,
        );

        let mut new_samples: Vec<f32> = vec![0.0; 48000];
        let count = soundtouch_sys::soundtouch_SoundTouch_receiveSamples(
            &mut soundtouch as *mut _ as *mut c_void,
            new_samples.as_mut_ptr(),
            new_samples.len() as u32,
        );

        for sample in samples.iter().take(100) {
            println!("{}", sample);
        }
        println!();
        for sample in new_samples.iter().take(100) {
            println!("{}", sample);
        }
        println!("Count: {}", count);
        println!(
            "Waiting: {:?}",
            soundtouch_sys::soundtouch_numSamples(&soundtouch as *const soundtouch_SoundTouch)
        );
    }
    return Ok(());

    let multiplexer = Multiplexer::new();
    multiplexer.run().unwrap();
    Ok(())
}
