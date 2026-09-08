//! 系统声音律动：对默认输出设备开 `build_input_stream` 做 loopback（mac 走 Core Audio
//! process tap 聚合设备，macOS 14.2+；Win 走 WASAPI loopback），采样经 rustfft 出 128 段频谱。

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use rustfft::{Fft, FftPlanner, num_complex::Complex};

const FFT_SIZE: usize = 1024;
/// 频谱段数（喂给 `wallpaperRegisterAudioListener` 的数组长度）。
pub const BANDS: usize = 128;

/// 持有 loopback 音频流与 FFT 状态。丢弃即停流。
pub struct Audio {
    _stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>, // 最近 FFT_SIZE 个单声道采样
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>, // Hann 窗
}

/// 按采样类型 `T` 建 loopback 输入流，回调把多声道下混成单声道 f32 存进 `sink`。
/// 输出设备混音格式跨平台各异（mac 多 F32、Win WASAPI 可能 I16/U16），统一转 f32。
fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    sink: Arc<Mutex<Vec<f32>>>,
    channels: usize,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            if let Ok(mut buf) = sink.lock() {
                let mut i = 0;
                while i + channels <= data.len() {
                    let mut m = 0.0f32;
                    for c in 0..channels {
                        m += f32::from_sample_(data[i + c]);
                    }
                    buf.push(m / channels as f32);
                    i += channels;
                }
                let len = buf.len();
                if len > FFT_SIZE {
                    buf.drain(0..len - FFT_SIZE);
                }
            }
        },
        |err| eprintln!("[audio] 流错误: {err}"),
        None,
    )
}

/// 启动系统声音捕获；失败（无设备/权限/旧系统/不支持格式）返回 None，律动静默降级。
pub fn start() -> Option<Audio> {
    let host = cpal::default_host();
    let device = host.default_output_device()?; // 输出设备 → cpal 走 loopback
    let cfg = device.default_output_config().ok()?;
    let channels = cfg.channels() as usize;
    let stream_config: cpal::StreamConfig = cfg.config();

    let samples = Arc::new(Mutex::new(vec![0.0f32; FFT_SIZE]));
    // 按设备实际采样格式分派——写死 f32 会在非 F32 设备上 BuildStreamError→静默无律动。
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(&device, stream_config, samples.clone(), channels)
        }
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(&device, stream_config, samples.clone(), channels)
        }
        cpal::SampleFormat::U16 => {
            build_stream::<u16>(&device, stream_config, samples.clone(), channels)
        }
        other => {
            eprintln!("[audio] 不支持的采样格式 {other:?}，律动降级");
            return None;
        }
    }
    .ok()?;
    stream.play().ok()?;

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let window = (0..FFT_SIZE)
        .map(|i| {
            (std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32)
                .sin()
                .powi(2)
        })
        .collect();

    Some(Audio {
        _stream: stream,
        samples,
        fft,
        window,
    })
}

impl Audio {
    /// 计算当前 128 段归一化频谱（0..1，对数频率分桶）。不在此做时间平滑——
    /// 壳侧输出即时值，视觉平滑单独交给 JS 侧一级 EMA，避免两级串联平滑糊掉律动、增延迟。
    pub fn bands(&self) -> [f32; BANDS] {
        let buf = match self.samples.lock() {
            Ok(b) => b.clone(),
            Err(_) => return [0.0; BANDS],
        };
        let mut c: Vec<Complex<f32>> = (0..FFT_SIZE)
            .map(|i| Complex::new(buf.get(i).copied().unwrap_or(0.0) * self.window[i], 0.0))
            .collect();
        self.fft.process(&mut c);

        let half = FFT_SIZE / 2;
        let mut out = [0.0f32; BANDS];
        for (b, slot) in out.iter_mut().enumerate() {
            // 对数频率分桶：低频细、高频粗，贴合听感
            let lo = (half as f32).powf(b as f32 / BANDS as f32) as usize;
            let hi = ((half as f32).powf((b + 1) as f32 / BANDS as f32) as usize).max(lo + 1);
            let (lo, hi) = (lo.min(half - 1), hi.min(half));
            let mag: f32 = c[lo..hi].iter().map(|x| x.norm()).sum::<f32>() / (hi - lo) as f32;
            *slot = (mag * 0.025).min(1.0);
        }
        out
    }
}
