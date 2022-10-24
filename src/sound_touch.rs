// Available setting IDs for the 'setSetting' & 'get_setting' functions:

use std::ffi::{c_int, c_void};

use soundtouch_sys::{soundtouch_SoundTouch, uint};

enum Setting {
    /// Enable/disable anti-alias filter in pitch transposer (0 = disable)
    UseAaFilter,

    /// Pitch transposer anti-alias filter length (8 .. 128 taps, default = 32)
    AaFilterLength,

    /// Enable/disable quick seeking algorithm in tempo changer routine
    /// (enabling quick seeking lowers CPU utilization but causes a minor sound
    ///  quality compromising)
    UseQuickseek,

    /// Time-stretch algorithm single processing sequence length in milliseconds. This determines
    /// to how long sequences the original sound is chopped in the time-stretch algorithm.
    /// See "STTypes.h" or README for more information.
    SequenceMs,

    /// Time-stretch algorithm seeking window length in milliseconds for algorithm that finds the
    /// best possible overlapping location. This determines from how wide window the algorithm
    /// may look for an optimal joining location when mixing the sound sequences back together.
    /// See "STTypes.h" or README for more information.
    SeekwindowMs,

    /// Time-stretch algorithm overlap length in milliseconds. When the chopped sound sequences
    /// are mixed back together, to form a continuous sound stream, this parameter defines over
    /// how long period the two consecutive sequences are let to overlap each other.
    /// See "STTypes.h" or README for more information.
    OverlapMs,

    /// Call "getSetting" with this ID to query processing sequence size in samples.
    /// This value gives approximate value of how many input samples you'll need to
    /// feed into SoundTouch after initial buffering to get out a new batch of
    /// output samples.
    ///
    /// This value does not include initial buffering at beginning of a new processing
    /// stream, use INITIAL_LATENCY to get the initial buffering size.
    ///
    /// Notices:
    /// - This is read-only parameter, i.e. setSetting ignores this parameter
    /// - This parameter value is not pub constant but change depending on
    ///   tempo/pitch/rate/samplerate settings.
    NominalInputSequence,

    /// Call "getSetting" with this ID to query nominal average processing output
    /// size in samples. This value tells approcimate value how many output samples
    /// SoundTouch outputs once it does DSP processing run for a batch of input samples.
    ///
    /// Notices:
    /// - This is read-only parameter, i.e. setSetting ignores this parameter
    /// - This parameter value is not pub constant but change depending on
    ///   tempo/pitch/rate/samplerate settings.
    NominalOutputSequence,

    /// Call "getSetting" with this ID to query initial processing latency, i.e.
    /// approx. how many samples you'll need to enter to SoundTouch pipeline before
    /// you can expect to get first batch of ready output samples out.
    ///
    /// After the first output batch, you can then expect to get approx.
    /// NOMINAL_OUTPUT_SEQUENCE ready samples out for every
    /// NOMINAL_INPUT_SEQUENCE samples that you enter into SoundTouch.
    ///
    /// Example:
    ///     processing with parameter -tempo=5
    ///     => initial latency = 5509 samples
    ///        input sequence  = 4167 samples
    ///        output sequence = 3969 samples
    ///
    /// Accordingly, you can expect to feed in approx. 5509 samples at beginning of
    /// the stream, and then you'll get out the first 3969 samples. After that, for
    /// every approx. 4167 samples that you'll put in, you'll receive again approx.
    /// 3969 samples out.
    ///
    /// This also means that average latency during stream processing is
    /// INITIAL_LATENCY-OUTPUT_SEQUENCE/2, in the above example case 5509-3969/2
    /// = 3524 samples
    ///
    /// Notices:
    /// - This is read-only parameter, i.e. setSetting ignores this parameter
    /// - This parameter value is not pub constant but change depending on
    ///   tempo/pitch/rate/samplerate settings.
    InitialLatency,
}

impl Setting {
    fn as_c_int(&self) -> c_int {
        match self {
            Setting::UseAaFilter => 0,
            Setting::AaFilterLength => 1,
            Setting::UseQuickseek => 2,
            Setting::SequenceMs => 3,
            Setting::SeekwindowMs => 4,
            Setting::OverlapMs => 5,
            Setting::NominalInputSequence => 6,
            Setting::NominalOutputSequence => 7,
            Setting::InitialLatency => 8,
        }
    }
}

pub struct SoundTouch {
    inner: soundtouch_SoundTouch,
}

unsafe impl Send for SoundTouch {}

impl Default for SoundTouch {
    fn default() -> Self {
        let inner = unsafe { soundtouch_SoundTouch::new() };
        Self { inner }
    }
}

impl SoundTouch {
    pub fn new() -> Self {
        let inner = unsafe { soundtouch_SoundTouch::new() };
        Self { inner }
    }

    pub fn set_channels(&mut self, num_channels: u32) {
        unsafe { self.inner.setChannels(num_channels) }
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        unsafe {
            self.inner.setSampleRate(sample_rate);
        }
    }

    pub fn set_tempo(&mut self, tempo: f64) {
        unsafe {
            self.inner.setTempo(tempo);
        }
    }

    pub fn set_setting(&mut self, setting: Setting, value: i64) {
        unsafe {
            self.inner.setSetting(setting.as_c_int(), value as c_int);
        }
    }

    // Adds 'numSamples' pcs of samples from the 'samples' memory position into
    // the input of the object. Notice that sample rate _has_to_ be set before
    // calling this function, otherwise throws a runtime_error exception.
    pub fn put_samples(&mut self, samples: &[f32], num_samples: usize) {
        unsafe {
            soundtouch_sys::soundtouch_SoundTouch_putSamples(
                &mut self.inner as *mut _ as *mut c_void,
                samples.as_ptr(),
                num_samples as uint,
            );
        }
    }

    // Output samples from beginning of the sample buffer. Copies requested samples to
    // output buffer and removes them from the sample buffer. If there are less than
    // 'numsample' samples in the buffer, returns all that available.
    //
    // \return Number of samples returned.
    pub fn receive_samples(&mut self, samples: &mut [f32], max_samples: usize) -> usize {
        unsafe {
            soundtouch_sys::soundtouch_SoundTouch_receiveSamples(
                &mut self.inner as *mut _ as *mut c_void,
                samples.as_mut_ptr(),
                max_samples as uint,
            ) as usize
        }
    }

    pub fn num_samples(&self) -> usize {
        unsafe {
            println!("{:?}", (*self.inner._base.output).vtable_);
            0
        }
    }
}
