pub mod dsp;
pub mod ring;

#[cfg(not(target_os = "android"))]
pub mod capture;

#[cfg(target_os = "android")]
pub mod jni;

use rtrb::Producer;
use std::sync::mpsc;
use std::thread::JoinHandle;

/// 采集信息,由采集源回填。平台共享(桌面 cpal / Android Kotlin 回传)。
#[derive(Debug, Clone)]
pub struct CaptureInfo {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

/// 统一音频采集入口。平台分支:
/// - 桌面:cpal loopback 线程,主动采集。
/// - Android:被动模式,Kotlin 驱动 AudioRecord 经 JNI push 样本(P3 实现)。
pub fn start_audio_source(
    producer: Producer<f32>,
) -> (JoinHandle<()>, mpsc::Sender<()>, mpsc::Receiver<Result<CaptureInfo, String>>) {
    #[cfg(not(target_os = "android"))]
    {
        capture::start_loopback_thread(producer)
    }
    #[cfg(target_os = "android")]
    {
        // P3 将实现真实采集:Kotlin AudioCaptureService 经 JNI push_samples 到 producer,
        // 并回填 CaptureInfo(sample_rate 由 Kotlin AudioRecord 配置决定)。
        let _ = producer;
        let (stop_tx, _stop_rx) = mpsc::channel::<()>();
        let (info_tx, info_rx) = mpsc::channel::<Result<CaptureInfo, String>>();
        let _ = info_tx.send(Err("Android audio source not implemented until P3".into()));
        let handle = std::thread::Builder::new()
            .name("audio-capture-stub".into())
            .spawn(|| {})
            .expect("spawn stub thread");
        (handle, stop_tx, info_rx)
    }
}
