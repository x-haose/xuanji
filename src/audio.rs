//! 系统声音律动：对默认输出设备开 `build_input_stream`，cpal 自动建 Core Audio
//! process tap 聚合设备做 loopback（macOS 14.2+）；采样经 rustfft 出 128 段频谱。

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustfft::{Fft, FftPlanner, num_complex::Complex};

const FFT_SIZE: usize = 1024;
/// 频谱段数（喂给 `wallpaperRegisterAudioListener` 的数组长度）。
pub const BANDS: usize = 128;

/// 持有 loopback 音频流与 FFT 状态。丢弃即停流。
pub struct Audio {
    _stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>, // 最近 FFT_SIZE 个单声道采样
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,            // Hann 窗
    smooth: Mutex<[f32; BANDS]>, // 平滑(起快落慢)，视觉不抖
}

/// 启动系统声音捕获；失败（无设备/权限/旧系统）返回 None，律动静默降级。
pub fn start() -> Option<Audio> {
    let host = cpal::default_host();
    let device = host.default_output_device()?; // 输出设备 → cpal 走 loopback
    let cfg = device.default_output_config().ok()?;
    let channels = cfg.channels() as usize;
    let stream_config: cpal::StreamConfig = cfg.config();

    let samples = Arc::new(Mutex::new(vec![0.0f32; FFT_SIZE]));
    let sink = samples.clone();
    let stream = device
        .build_input_stream(
            stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut buf) = sink.lock() {
                    let mut i = 0;
                    while i + channels <= data.len() {
                        let mut m = 0.0f32;
                        for c in 0..channels {
                            m += data[i + c];
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
        smooth: Mutex::new([0.0; BANDS]),
    })
}

impl Audio {
    /// 计算当前 128 段归一化频谱（0..1，对数频率分桶 + 平滑）。
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

        // 平滑：起快(0.5)落慢(0.15)，视觉不闪
        if let Ok(mut sm) = self.smooth.lock() {
            for b in 0..BANDS {
                let k = if out[b] > sm[b] { 0.5 } else { 0.15 };
                sm[b] += (out[b] - sm[b]) * k;
                out[b] = sm[b];
            }
        }
        out
    }
}
