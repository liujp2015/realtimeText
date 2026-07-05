//! Android JNI 桥:Kotlin AudioCaptureService 经此 push PCM 样本到 Rust pipeline。
//! 仅 Android 编译。Kotlin 侧类:com.realtimesubtitle.tool.AudioBridge。

use jni::objects::{JByteArray, JClass};
use jni::JNIEnv;
use parking_lot::Mutex;
use rtrb::Producer;

/// 全局音频 producer。`session_start` 写入,`pushSamples` JNI 读取。
/// `Producer::push` 是 `&mut self`(rtrb 单写者),故用 `Mutex<Option<Producer>>`
/// 持有,Kotlin capture 线程经 `as_mut()` 取 `&mut` push(单线程,无竞争)。
static AUDIO_PRODUCER: Mutex<Option<Producer<f32>>> = Mutex::new(None);

/// `session_start` 调用:存入 producer 供 Kotlin push。
pub fn set_producer(producer: Producer<f32>) {
    *AUDIO_PRODUCER.lock() = Some(producer);
}

/// `session_stop` 调用:清除 producer,停止接收。
pub fn clear_producer() {
    *AUDIO_PRODUCER.lock() = None;
}

/// Kotlin 调用:push PCM s16le mono bytes → Rust 转 f32 → producer。
/// 对应 Kotlin: `external fun pushSamples(samples: ByteArray)`
#[no_mangle]
pub extern "system" fn Java_com_realtimesubtitle_tool_AudioBridge_pushSamples(
    env: JNIEnv,
    _class: JClass,
    samples: JByteArray,
) {
    let bytes = match env.convert_byte_array(&samples) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("pushSamples convert_byte_array: {e}");
            return;
        }
    };
    let mut guard = AUDIO_PRODUCER.lock();
    let Some(producer) = guard.as_mut() else {
        return;
    };
    for chunk in bytes.chunks_exact(2) {
        let s = i16::from_le_bytes([chunk[0], chunk[1]]);
        let f = s as f32 / 32768.0;
        let _ = producer.push(f);
    }
}
