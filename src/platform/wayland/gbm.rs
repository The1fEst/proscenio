use std::ffi::{c_int, c_void};
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};

const GBM_BO_USE_RENDERING: u32 = 1 << 2;

#[link(name = "gbm")]
unsafe extern "C" {
    fn gbm_create_device(fd: c_int) -> *mut c_void;
    fn gbm_device_destroy(device: *mut c_void);
    fn gbm_bo_create_with_modifiers2(
        device: *mut c_void,
        width: u32,
        height: u32,
        format: u32,
        modifiers: *const u64,
        count: u32,
        flags: u32,
    ) -> *mut c_void;
    fn gbm_bo_destroy(bo: *mut c_void);
    fn gbm_bo_get_plane_count(bo: *mut c_void) -> c_int;
    fn gbm_bo_get_fd_for_plane(bo: *mut c_void, plane: c_int) -> c_int;
    fn gbm_bo_get_stride_for_plane(bo: *mut c_void, plane: c_int) -> u32;
    fn gbm_bo_get_offset(bo: *mut c_void, plane: c_int) -> u32;
    fn gbm_bo_get_modifier(bo: *mut c_void) -> u64;
}

pub struct Plane {
    pub fd: OwnedFd,
    pub stride: u32,
    pub offset: u32,
}

pub struct Buffer {
    pub width: u32,
    pub height: u32,
    pub fourcc: u32,
    pub modifier: u64,
    pub planes: Vec<Plane>,
}

pub struct Gbm {
    device: *mut c_void,
    _node: File,
}

impl Gbm {
    pub fn open(node: &Path) -> Option<Self> {
        let node = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(node)
            .ok()?;
        let device = unsafe { gbm_create_device(node.as_raw_fd()) };
        (!device.is_null()).then_some(Gbm {
            device,
            _node: node,
        })
    }

    pub fn allocate(
        &self,
        width: u32,
        height: u32,
        fourcc: u32,
        modifiers: &[u64],
    ) -> Option<Buffer> {
        if modifiers.is_empty() {
            return None;
        }
        let bo = unsafe {
            gbm_bo_create_with_modifiers2(
                self.device,
                width,
                height,
                fourcc,
                modifiers.as_ptr(),
                modifiers.len() as u32,
                GBM_BO_USE_RENDERING,
            )
        };
        if bo.is_null() {
            return None;
        }
        let count = unsafe { gbm_bo_get_plane_count(bo) };
        let planes: Vec<Plane> = (0..count)
            .map_while(|plane| {
                let fd = unsafe { gbm_bo_get_fd_for_plane(bo, plane) };
                (fd >= 0).then(|| Plane {
                    fd: unsafe { OwnedFd::from_raw_fd(fd) },
                    stride: unsafe { gbm_bo_get_stride_for_plane(bo, plane) },
                    offset: unsafe { gbm_bo_get_offset(bo, plane) },
                })
            })
            .collect();
        let modifier = unsafe { gbm_bo_get_modifier(bo) };
        unsafe { gbm_bo_destroy(bo) };
        (count > 0 && planes.len() == count as usize).then_some(Buffer {
            width,
            height,
            fourcc,
            modifier,
            planes,
        })
    }
}

impl Drop for Gbm {
    fn drop(&mut self) {
        unsafe { gbm_device_destroy(self.device) };
    }
}

pub fn numbers(device: u64) -> (u64, u64) {
    (
        ((device >> 32) & 0xffff_f000) | ((device >> 8) & 0xfff),
        ((device >> 12) & 0xffff_ff00) | (device & 0xff),
    )
}

pub fn render_node(device: u64) -> Option<PathBuf> {
    let (major, minor) = numbers(device);
    std::fs::read_dir(format!("/sys/dev/char/{major}:{minor}/device/drm"))
        .ok()?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|name| name.starts_with("renderD"))
        .map(|name| Path::new("/dev/dri").join(name))
}
