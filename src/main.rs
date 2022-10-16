use pipewire::{
    properties,
    spa::Direction,
    stream::{ListenerBuilderT, StreamFlags},
    Context, MainLoop,
};

fn main() -> anyhow::Result<()> {
    let mainloop = MainLoop::new()?;
    let mut stream = pipewire::stream::Stream::<()>::simple(
        &mainloop,
        "audio-test",
        properties! {
            *pipewire::keys::MEDIA_TYPE => "Audio",
            *pipewire::keys::MEDIA_CLASS => "Audio/Sink",
            *pipewire::keys::MEDIA_CATEGORY => "Duplex",
            *pipewire::keys::MEDIA_ROLE => "DSP",
        },
    )
    .state_changed(|old, new| {
        println!("State changed: {:?} -> {:?}", old, new);
    })
    .process(|_stream, _user_data| {
        println!("On frame");
    })
    .create()?;

    stream.connect(
        Direction::Input,
        None,
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
        &mut [],
    )?;

    mainloop.run();

    Ok(())
}
