use anyhow::Ok;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;

fn main() -> anyhow::Result<()> {
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
    let output_device = host
        .default_output_device()
        .expect("Failed to find output device");

    let config: cpal::StreamConfig = input_device.default_input_config()?.into();

    let ring = HeapRb::new(4096);
    let (mut producer, mut consumer) = ring.split();
    let handle_input = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        if data.iter().all(|f| f.abs() < 0.01) {
            return;
        }
        println!("len: {}", data.len());
        println!("sum: {}", data.iter().sum::<f32>());
        for &sample in data {
            producer.push(sample);
        }
    };
    let handle_output = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        for sample in data {
            *sample = consumer.pop().unwrap_or(0.0);
        }
    };

    let input_stream = input_device.build_input_stream(&config, handle_input, err_fn)?;
    let output_stream = output_device.build_output_stream(&config, handle_output, err_fn)?;

    input_stream.play()?;
    output_stream.play()?;
    std::thread::sleep(std::time::Duration::from_secs(300));

    Ok(())
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("an error occurred on stream: {}", err);
}
