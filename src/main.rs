use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Ok};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{Consumer, HeapRb, Producer};

struct Input {
    consumer: Consumer<f32, Arc<HeapRb<f32>>>,
}

impl Input {
    fn new(input_device: &cpal::Device) -> (Self, cpal::Stream) {
        let buffer = HeapRb::new(4096);
        let (mut producer, mut consumer) = buffer.split();

        let input_handler = move |data: &[f32], _: &cpal::InputCallbackInfo| {
            if data.iter().all(|f| f.abs() < 0.01) {
                return;
            }
            println!("len: {}", data.len());
            println!("sum: {}", data.iter().sum::<f32>());
            for &sample in data {
                producer.push(sample);
            }
        };
        let config = input_device.default_input_config().unwrap().into();
        let stream = input_device
            .build_input_stream(&config, input_handler, err_fn)
            .unwrap();
        (Self { consumer }, stream)
    }
}

struct Multiplexer {
    host: cpal::Host,
    inputs: Arc<Mutex<Vec<Input>>>,
    input_streams: Vec<cpal::Stream>,
}

impl Multiplexer {
    fn new(input_count: usize) -> Self {
        let host = cpal::host_from_id(
            cpal::available_hosts()
                .into_iter()
                .find(|id| *id == cpal::HostId::Jack)
                .expect("No jack server found"),
        )
        .expect("Found but failed to talk to jack server");

        let input_device = host
            .default_input_device()
            .expect("Failed to find input device");
        let mut inputs = Vec::new();
        let mut input_streams = Vec::new();

        for _index in 0..input_count {
            let (input, stream) = Input::new(&input_device);
            inputs.push(input);
            input_streams.push(stream);
        }
        let inputs = Arc::new(Mutex::new(inputs));
        Self {
            host,
            inputs,
            input_streams,
        }
    }

    fn run(&self) -> anyhow::Result<()> {
        for stream in &self.input_streams {
            stream.play()?;
        }
        let inputs = self.inputs.clone();
        let handle_output = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let inputs = &mut inputs.lock().unwrap();
            let best_candidate = inputs.iter_mut().find(|input| !input.consumer.is_empty());
            let consumer = match best_candidate {
                Some(input) => &mut input.consumer,
                None => {
                    data.fill(0.0);
                    return;
                }
            };
            for sample in data {
                *sample = consumer.pop().unwrap_or(0.0);
            }
        };
        let output_device = self
            .host
            .default_output_device()
            .expect("Failed to find output device");
        let config = output_device.default_output_config()?.into();

        let output_stream = output_device.build_output_stream(&config, handle_output, err_fn)?;
        output_stream.play()?;

        std::thread::sleep(std::time::Duration::from_secs(300));

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let multiplexer = Multiplexer::new(2);
    multiplexer.run();

    Ok(())
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("an error occurred on stream: {}", err);
}
