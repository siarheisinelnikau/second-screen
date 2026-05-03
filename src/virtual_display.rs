//! IddCx client and strict wire decoder. No device is opened except on explicit source activation.
use crate::capture::{ExtendedDisplayProvider, FramePoll, FrameSource};
use image::RgbImage;
use std::sync::{Arc, Mutex, Weak};

pub const PROTOCOL: u32 = 0x0001_0000;
pub const HEADER: usize = 64;
pub const MAX_PIXELS: usize = 1920 * 1080 * 4;
pub const QUERY: u32 = 0x0022_e000;
pub const CREATE: u32 = 0x0022_e004;
pub const FRAME: u32 = 0x0022_e008;
pub const REMOVE: u32 = 0x0022_e00c;

pub trait DriverTransport {
    fn ioctl(&mut self, code: u32, request: &[u8; 32], output: &mut [u8]) -> Result<usize, String>;
}
fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}
fn u64_at(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}
pub fn frame_request(generation: u64, sequence: u64) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[..4].copy_from_slice(&PROTOCOL.to_le_bytes());
    bytes[4..8].copy_from_slice(&32u32.to_le_bytes());
    bytes[8..16].copy_from_slice(&generation.to_le_bytes());
    bytes[16..24].copy_from_slice(&sequence.to_le_bytes());
    bytes
}
fn create_request(width: u32, height: u32, fps: u32) -> [u8; 32] {
    let mut bytes = frame_request(0, 0);
    bytes[8..12].copy_from_slice(&width.to_le_bytes());
    bytes[12..16].copy_from_slice(&height.to_le_bytes());
    bytes[16..20].copy_from_slice(&fps.to_le_bytes());
    bytes
}
fn validate_header(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < HEADER || bytes.len() > HEADER + MAX_PIXELS {
        return Err("Invalid virtual-display response size".into());
    }
    if u32_at(bytes, 0) != PROTOCOL
        || u32_at(bytes, 4) != HEADER as u32
        || u32_at(bytes, 40) != 1
        || bytes[52..64].iter().any(|b| *b != 0)
    {
        return Err("Incompatible virtual-display protocol/header".into());
    }
    Ok(())
}
pub fn decode_frame(
    bytes: &[u8],
    generation: u64,
    after: u64,
    width: u32,
    height: u32,
) -> Result<(FramePoll, u64), String> {
    validate_header(bytes)?;
    if u64_at(bytes, 8) != generation
        || u32_at(bytes, 24) != width
        || u32_at(bytes, 28) != height
        || !(320..=1920).contains(&width)
        || !(240..=1080).contains(&height)
        || u32_at(bytes, 32) != width * 4
    {
        return Err("Virtual-display generation, dimensions or stride changed".into());
    }
    let sequence = u64_at(bytes, 16);
    let payload = u32_at(bytes, 36) as usize;
    let state = u32_at(bytes, 44);
    if bytes.len() != HEADER + payload {
        return Err("Truncated or trailing virtual-display frame data".into());
    }
    if state != 1 {
        if payload != 0 {
            return Err("Non-frame response contains pixel data".into());
        }
        return match state {
            0 if u32_at(bytes, 48) == 0 => Ok((FramePoll::Unchanged, after)),
            2 if u32_at(bytes, 48) == 0 => Ok((FramePoll::Inactive, after)),
            3 => Err(format!(
                "Virtual-display GPU worker failed (HRESULT 0x{:08x}); stop and reconnect",
                u32_at(bytes, 48)
            )),
            _ => Err("Unknown virtual-display frame state".into()),
        };
    }
    if sequence <= after
        || u32_at(bytes, 48) != 0
        || payload != width as usize * height as usize * 4
    {
        return Err("Invalid virtual-display frame sequence or payload".into());
    }
    let rgb = bytes[HEADER..]
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[2], pixel[1], pixel[0]])
        .collect();
    let image =
        RgbImage::from_raw(width, height, rgb).ok_or("Cannot decode virtual-display pixels")?;
    Ok((FramePoll::Frame(image), sequence))
}

struct Session {
    transport: Box<dyn DriverTransport + Send>,
    generation: u64,
    sequence: u64,
    width: u32,
    height: u32,
    active: bool,
    output: Vec<u8>,
}
impl Session {
    fn create(
        mut transport: Box<dyn DriverTransport + Send>,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Result<Self, String> {
        if !(320..=1920).contains(&width)
            || !(240..=1080).contains(&height)
            || !(5..=20).contains(&fps)
        {
            return Err("Unsupported virtual-display mode".into());
        }
        let mut header = [0; HEADER];
        let n = transport.ioctl(QUERY, &frame_request(0, 0), &mut header)?;
        if n != HEADER {
            return Err("Invalid driver capability reply".into());
        }
        validate_header(&header)?;
        if u32_at(&header, 44) != 0 {
            return Err("Virtual-display adapter is not ready".into());
        }
        if u64_at(&header, 8) != 0
            || u64_at(&header, 16) != 0
            || u32_at(&header, 36) != 0
            || u32_at(&header, 48) != 0
            || u32_at(&header, 24) < width
            || u32_at(&header, 28) < height
        {
            return Err("Invalid virtual-display capability fields".into());
        }
        let n = transport.ioctl(CREATE, &create_request(width, height, fps), &mut header)?;
        // Closing the owning file removes a monitor even if a malformed create reply fails validation.
        if n != HEADER {
            return Err("Invalid driver monitor-create reply".into());
        }
        validate_header(&header)?;
        let generation = u64_at(&header, 8);
        if generation == 0
            || u32_at(&header, 24) != width
            || u32_at(&header, 28) != height
            || u32_at(&header, 44) != 0
            || u32_at(&header, 36) != 0
            || u64_at(&header, 16) != 0
            || u32_at(&header, 32) != width * 4
            || u32_at(&header, 48) != 0
        {
            return Err("Driver created an incompatible monitor".into());
        }
        Ok(Self {
            transport,
            generation,
            sequence: 0,
            width,
            height,
            active: true,
            output: vec![0; HEADER + width as usize * height as usize * 4],
        })
    }
    fn poll(&mut self) -> Result<FramePoll, String> {
        if !self.active {
            return Err("Virtual display was removed".into());
        }
        let n = self.transport.ioctl(
            FRAME,
            &frame_request(self.generation, self.sequence),
            &mut self.output,
        )?;
        if n > self.output.len() {
            return Err("Driver returned an invalid response length".into());
        }
        let (frame, sequence) = decode_frame(
            &self.output[..n],
            self.generation,
            self.sequence,
            self.width,
            self.height,
        )?;
        self.sequence = sequence;
        Ok(frame)
    }
    fn remove(&mut self) -> Result<(), String> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        self.transport
            .ioctl(REMOVE, &frame_request(self.generation, 0), &mut [0; HEADER])
            .map(|_| ())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}
struct VirtualSource(Arc<Mutex<Session>>);
impl FrameSource for VirtualSource {
    fn frame(&mut self) -> Result<RgbImage, String> {
        match self.poll()? {
            FramePoll::Frame(frame) => Ok(frame),
            _ => Err("Waiting for virtual desktop output".into()),
        }
    }
    fn poll(&mut self) -> Result<FramePoll, String> {
        self.0
            .lock()
            .map_err(|_| "Virtual-display session failed")?
            .poll()
    }
}

pub struct Provider {
    session: Weak<Mutex<Session>>,
    fps: u32,
}
impl Default for Provider {
    fn default() -> Self {
        Self::new(10)
    }
}
impl Provider {
    pub fn new(fps: u8) -> Self {
        Self {
            session: Weak::new(),
            fps: fps.into(),
        }
    }
    fn create_with(
        &mut self,
        transport: Box<dyn DriverTransport + Send>,
        width: u32,
        height: u32,
    ) -> Result<Box<dyn FrameSource>, String> {
        if self.session.upgrade().is_some() {
            return Err("Provider already owns a virtual monitor".into());
        }
        let session = Arc::new(Mutex::new(Session::create(
            transport, width, height, self.fps,
        )?));
        self.session = Arc::downgrade(&session);
        Ok(Box::new(VirtualSource(session)))
    }
}
impl ExtendedDisplayProvider for Provider {
    fn create_monitor(&mut self, width: u32, height: u32) -> Result<Box<dyn FrameSource>, String> {
        #[cfg(windows)]
        {
            self.create_with(Box::new(device::Device::open()?), width, height)
        }
        #[cfg(not(windows))]
        {
            let _ = (width, height);
            Err("Virtual display requires Windows".into())
        }
    }
    fn remove_monitor(&mut self) -> Result<(), String> {
        if let Some(session) = self.session.upgrade() {
            session
                .lock()
                .map_err(|_| "Virtual-display session failed")?
                .remove()?;
        }
        Ok(())
    }
}

#[cfg(windows)]
mod device {
    use super::*;
    use std::{
        fs::{File, OpenOptions},
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
        ptr::null,
    };
    use windows_sys::{
        Win32::{Devices::DeviceAndDriverInstallation::*, System::IO::DeviceIoControl},
        core::GUID,
    };
    const INTERFACE: GUID = GUID {
        data1: 0x89e573d9,
        data2: 0x1361,
        data3: 0x48d7,
        data4: [0xbd, 0x37, 0x3c, 0x8e, 0x55, 0x98, 0xef, 0x4a],
    };
    pub struct Device(File);
    impl Device {
        pub fn open() -> Result<Self, String> {
            for _ in 0..3 {
                let mut length = 0;
                // SAFETY: GUID is static; sizes/pointers refer to owned bounded buffers.
                let result = unsafe {
                    CM_Get_Device_Interface_List_SizeW(
                        &mut length,
                        &INTERFACE,
                        null(),
                        CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
                    )
                };
                if result != CR_SUCCESS || length > 65536 {
                    return Err(format!(
                        "Cannot enumerate SecondScreen driver (CONFIGRET {result})"
                    ));
                }
                if length <= 1 {
                    return Err("SecondScreen virtual-display driver is not installed. See the driver build guide; this app never installs it.".into());
                }
                let mut buffer = vec![0u16; length as usize];
                let result = unsafe {
                    CM_Get_Device_Interface_ListW(
                        &INTERFACE,
                        null(),
                        buffer.as_mut_ptr(),
                        length,
                        CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
                    )
                };
                if result == CR_BUFFER_SMALL {
                    continue;
                }
                if result != CR_SUCCESS {
                    return Err(format!(
                        "Cannot read driver interface list (CONFIGRET {result})"
                    ));
                }
                let paths: Vec<_> = buffer
                    .split(|c| *c == 0)
                    .filter(|s| !s.is_empty())
                    .collect();
                if paths.len() != 1 {
                    return Err("Expected exactly one SecondScreen virtual adapter".into());
                }
                let path = std::ffi::OsString::from_wide(paths[0]);
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .share_mode(0)
                    .open(path)
                    .map_err(|e| format!("Cannot open virtual-display driver: {e}"))?;
                return Ok(Self(file));
            }
            Err("Driver list changed during enumeration; retry".into())
        }
    }
    impl DriverTransport for Device {
        fn ioctl(
            &mut self,
            code: u32,
            request: &[u8; 32],
            output: &mut [u8],
        ) -> Result<usize, String> {
            let mut returned = 0;
            // SAFETY: file owns a live handle, METHOD_BUFFERED copies a 32-byte request
            // and writes at most the explicit output capacity. No pointers cross the ABI.
            let ok = unsafe {
                DeviceIoControl(
                    self.0.as_raw_handle(),
                    code,
                    request.as_ptr().cast(),
                    32,
                    output.as_mut_ptr().cast(),
                    output.len() as u32,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(format!(
                    "Virtual-display IOCTL 0x{code:08x} failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(returned as usize)
        }
    }
    use std::os::windows::ffi::OsStringExt;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header(generation: u64, state: u32, sequence: u64) -> Vec<u8> {
        let mut bytes = vec![0; HEADER];
        bytes[..4].copy_from_slice(&PROTOCOL.to_le_bytes());
        bytes[4..8].copy_from_slice(&64u32.to_le_bytes());
        bytes[8..16].copy_from_slice(&generation.to_le_bytes());
        bytes[16..24].copy_from_slice(&sequence.to_le_bytes());
        for (offset, value) in [(24, 320u32), (28, 240), (32, 1280), (40, 1), (44, state)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
    #[test]
    fn rejects_stale_malformed_truncated_and_hostile_frame_headers() {
        let pending = header(7, 0, 0);
        assert!(matches!(
            decode_frame(&pending, 7, 0, 320, 240).unwrap().0,
            FramePoll::Unchanged
        ));
        assert!(decode_frame(&pending, 8, 0, 320, 240).is_err());
        for offset in [0, 4, 24, 28, 32, 36, 40, 44, 48, 52] {
            let mut bad = pending.clone();
            bad[offset] ^= 0xff;
            assert!(
                decode_frame(&bad, 7, 0, 320, 240).is_err(),
                "offset {offset}"
            );
        }
        assert!(decode_frame(&pending[..63], 7, 0, 320, 240).is_err());
        assert!(decode_frame(&header(7, 1, 1), 7, 0, 320, 240).is_err());
    }
    struct Fake {
        calls: Arc<Mutex<Vec<u32>>>,
    }
    impl DriverTransport for Fake {
        fn ioctl(
            &mut self,
            code: u32,
            request: &[u8; 32],
            output: &mut [u8],
        ) -> Result<usize, String> {
            self.calls.lock().unwrap().push(code);
            let response = header(
                if code == QUERY { 0 } else { 7 },
                if code == REMOVE { 2 } else { 0 },
                0,
            );
            if code == FRAME || code == REMOVE {
                assert_eq!(u64_at(request, 8), 7);
            }
            output[..HEADER].copy_from_slice(&response);
            Ok(HEADER)
        }
    }
    #[test]
    fn monitor_ownership_removal_is_idempotent_and_source_drop_removes() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut provider = Provider::new(10);
        let mut source = provider
            .create_with(
                Box::new(Fake {
                    calls: calls.clone(),
                }),
                320,
                240,
            )
            .unwrap();
        assert!(matches!(source.poll().unwrap(), FramePoll::Unchanged));
        provider.remove_monitor().unwrap();
        provider.remove_monitor().unwrap();
        assert!(source.poll().is_err());
        drop(source);
        assert_eq!(*calls.lock().unwrap(), [QUERY, CREATE, FRAME, REMOVE]);
        let source = provider
            .create_with(
                Box::new(Fake {
                    calls: calls.clone(),
                }),
                320,
                240,
            )
            .unwrap();
        drop(source);
        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| **c == REMOVE)
                .count(),
            2
        );
    }
    #[test]
    #[ignore = "Run scripts/test-driver-core.ps1 first; reads only its synthetic driver fixture"]
    fn decodes_actual_cpp_mailbox_fixture() {
        let bytes = std::fs::read(".test-output/driver-frame.bin").unwrap();
        let (frame, sequence) = decode_frame(&bytes, 7, 0, 320, 240).unwrap();
        let FramePoll::Frame(frame) = frame else {
            panic!("Expected generated pixels")
        };
        assert_eq!(sequence, 1);
        assert_eq!(frame.get_pixel(0, 0).0, [30, 20, 10]);
        assert_eq!(frame.get_pixel(319, 239).0, [30, 20, 10]);
        assert!(decode_frame(&bytes, 7, 1, 320, 240).is_err());
    }
}
