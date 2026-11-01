//! No source is constructed until a paired viewer has been approved by the host.
use image::{ExtendedColorType, RgbImage, codecs::jpeg::JpegEncoder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SourceKind {
    #[default]
    PrimaryMonitor,
    Synthetic,
    VirtualDisplay,
}

impl SourceKind {
    pub fn mode_label(self) -> &'static str {
        match self {
            Self::PrimaryMonitor => "Mirroring",
            Self::Synthetic => "Synthetic test pattern",
            Self::VirtualDisplay => "Virtual display (experimental)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub source: SourceKind,
    pub max_width: u32,
    pub max_height: u32,
    pub quality: u8,
    pub fps: u8,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            source: SourceKind::PrimaryMonitor,
            max_width: 1280,
            max_height: 720,
            quality: 75,
            fps: 10,
        }
    }
}

impl StreamConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.source == SourceKind::VirtualDisplay
            && !cfg!(all(windows, feature = "virtual-display"))
        {
            return Err("Build with --features virtual-display on Windows to enable the experimental driver backend".into());
        }
        if !(320..=1920).contains(&self.max_width)
            || !(240..=1080).contains(&self.max_height)
            || !(30..=90).contains(&self.quality)
            || !(5..=20).contains(&self.fps)
        {
            return Err("Stream settings exceed the supported limits".into());
        }
        Ok(())
    }
}

/// Provider owns the control connection; the returned source owns the monitor lease.
/// A provider must own monitor arrival/departure and return its own frame source.
pub trait ExtendedDisplayProvider {
    fn create_monitor(&mut self, width: u32, height: u32) -> Result<Box<dyn FrameSource>, String>;
    fn remove_monitor(&mut self) -> Result<(), String>;
}

pub trait FrameSource {
    fn frame(&mut self) -> Result<RgbImage, String>;
    fn poll(&mut self) -> Result<FramePoll, String> {
        self.frame().map(FramePoll::Frame)
    }
}

pub enum FramePoll {
    Frame(RgbImage),
    Unchanged,
    Inactive,
}

pub fn open(config: &StreamConfig) -> Result<Box<dyn FrameSource>, String> {
    config.validate()?;
    match config.source {
        SourceKind::VirtualDisplay => {
            #[cfg(all(windows, feature = "virtual-display"))]
            {
                crate::virtual_display::Provider::new(config.fps)
                    .create_monitor(config.max_width, config.max_height)
            }
            #[cfg(not(all(windows, feature = "virtual-display")))]
            {
                Err("Virtual display backend is not enabled in this build".into())
            }
        }
        SourceKind::Synthetic => Ok(Box::new(Synthetic {
            width: config.max_width,
            height: config.max_height,
            sequence: 0,
        })),
        SourceKind::PrimaryMonitor => {
            #[cfg(windows)]
            {
                Ok(Box::new(gdi::Primary::new(config)?))
            }
            #[cfg(not(windows))]
            {
                Err("Windows screen capture is unavailable on this platform".into())
            }
        }
    }
}

pub fn encode(frame: &RgbImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, quality)
        .encode(
            frame.as_raw(),
            frame.width(),
            frame.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())?;
    Ok(jpeg)
}

pub fn fit(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let scale = (max_width as f64 / width as f64)
        .min(max_height as f64 / height as f64)
        .min(1.0);
    (
        (width as f64 * scale).round().max(1.0) as u32,
        (height as f64 * scale).round().max(1.0) as u32,
    )
}

struct Synthetic {
    width: u32,
    height: u32,
    sequence: u32,
}
impl FrameSource for Synthetic {
    fn frame(&mut self) -> Result<RgbImage, String> {
        let phase = self.sequence.wrapping_mul(12);
        self.sequence = self.sequence.wrapping_add(1);
        Ok(RgbImage::from_fn(self.width, self.height, |x, y| {
            let bar = (x + phase) % self.width < self.width / 12;
            image::Rgb(if bar {
                [71, 225, 185]
            } else {
                [
                    (x * 140 / self.width) as u8,
                    (y * 100 / self.height) as u8,
                    55,
                ]
            })
        }))
    }
}

#[cfg(windows)]
mod gdi {
    use super::*;
    use std::{
        mem::{size_of, zeroed},
        ptr::null_mut,
    };
    use windows_sys::Win32::{
        Graphics::Gdi::*,
        UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN},
    };

    // GDI handles stay on the capture worker that created them. Drop restores selection.
    pub struct Primary {
        screen: HDC,
        memory: HDC,
        bitmap: HBITMAP,
        previous: HGDIOBJ,
        width: u32,
        height: u32,
        bounds: (u32, u32),
    }
    impl Primary {
        pub fn new(config: &StreamConfig) -> Result<Self, String> {
            // SAFETY: GetSystemMetrics has no pointer inputs; all handles are checked below.
            unsafe {
                let sw = GetSystemMetrics(SM_CXSCREEN);
                let sh = GetSystemMetrics(SM_CYSCREEN);
                if sw <= 0 || sh <= 0 {
                    return Err("No primary desktop is available".into());
                }
                let (width, height) =
                    fit(sw as u32, sh as u32, config.max_width, config.max_height);
                let mut result = Self {
                    screen: null_mut(),
                    memory: null_mut(),
                    bitmap: null_mut(),
                    previous: null_mut(),
                    width,
                    height,
                    bounds: (config.max_width, config.max_height),
                };
                result.screen = GetDC(null_mut());
                if result.screen.is_null() {
                    return Err("GetDC failed".into());
                }
                result.memory = CreateCompatibleDC(result.screen);
                result.bitmap = CreateCompatibleBitmap(result.screen, width as i32, height as i32);
                if result.memory.is_null() || result.bitmap.is_null() {
                    return Err("Cannot allocate capture bitmap".into());
                }
                result.previous = SelectObject(result.memory, result.bitmap);
                if result.previous.is_null() || result.previous as isize == -1 {
                    result.previous = null_mut();
                    return Err("Cannot select capture bitmap".into());
                }
                SetStretchBltMode(result.memory, HALFTONE);
                Ok(result)
            }
        }
    }
    impl FrameSource for Primary {
        fn frame(&mut self) -> Result<RgbImage, String> {
            // SAFETY: handles are owned here; buffer is width*height*4 and BITMAPINFO
            // requests a top-down 32-bit DIB. GetDIBits is called with bitmap deselected.
            unsafe {
                let sw = GetSystemMetrics(SM_CXSCREEN);
                let sh = GetSystemMetrics(SM_CYSCREEN);
                if sw <= 0 || sh <= 0 {
                    return Err("Desktop unavailable; stop and restart sharing".into());
                }
                if fit(sw as u32, sh as u32, self.bounds.0, self.bounds.1)
                    != (self.width, self.height)
                {
                    return Err("Display mode changed; stop and restart sharing".into());
                }
                if StretchBlt(
                    self.memory,
                    0,
                    0,
                    self.width as i32,
                    self.height as i32,
                    self.screen,
                    0,
                    0,
                    sw,
                    sh,
                    SRCCOPY | CAPTUREBLT,
                ) == 0
                {
                    return Err("Desktop capture failed (locked/protected desktop?)".into());
                }
                SelectObject(self.memory, self.previous);
                let mut info: BITMAPINFO = zeroed();
                info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
                info.bmiHeader.biWidth = self.width as i32;
                info.bmiHeader.biHeight = -(self.height as i32);
                info.bmiHeader.biPlanes = 1;
                info.bmiHeader.biBitCount = 32;
                info.bmiHeader.biCompression = BI_RGB;
                let mut bgra = vec![0u8; (self.width * self.height * 4) as usize];
                let rows = GetDIBits(
                    self.memory,
                    self.bitmap,
                    0,
                    self.height,
                    bgra.as_mut_ptr().cast(),
                    &mut info,
                    DIB_RGB_COLORS,
                );
                SelectObject(self.memory, self.bitmap);
                if rows != self.height as i32 {
                    return Err("Cannot read desktop pixels".into());
                }
                let rgb = bgra
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| [p[2], p[1], p[0]])
                    .collect();
                RgbImage::from_raw(self.width, self.height, rgb)
                    .ok_or_else(|| "Invalid capture dimensions".into())
            }
        }
    }
    impl Drop for Primary {
        fn drop(&mut self) {
            // SAFETY: release only successfully acquired resources, in reverse order.
            unsafe {
                if !self.memory.is_null() && !self.previous.is_null() {
                    SelectObject(self.memory, self.previous);
                }
                if !self.bitmap.is_null() {
                    DeleteObject(self.bitmap);
                }
                if !self.memory.is_null() {
                    DeleteDC(self.memory);
                }
                if !self.screen.is_null() {
                    ReleaseDC(null_mut(), self.screen);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_jpeg_round_trip_and_movement() {
        let c = StreamConfig {
            source: SourceKind::Synthetic,
            max_width: 320,
            max_height: 240,
            ..Default::default()
        };
        let mut source = open(&c).unwrap();
        let first = encode(&source.frame().unwrap(), 80).unwrap();
        let second = encode(&source.frame().unwrap(), 80).unwrap();
        let decoded = image::load_from_memory(&first).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (320, 240));
        assert_ne!(first, second);
        assert_eq!(fit(1920, 1200, 1280, 720), (1152, 720));
        assert_eq!(fit(800, 600, 1920, 1080), (800, 600));
    }
}
