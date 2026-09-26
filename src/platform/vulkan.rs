use std::cell::OnceCell;
use std::ffi::{c_char, c_void};
use std::ptr::{null, null_mut};

const API_VERSION_1_1: u32 = (1 << 22) | (1 << 12);
const STRUCTURE_TYPE_APPLICATION_INFO: i32 = 0;
const STRUCTURE_TYPE_INSTANCE_CREATE_INFO: i32 = 1;
const STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2: i32 = 1_000_059_001;
const STRUCTURE_TYPE_FORMAT_PROPERTIES_2: i32 = 1_000_059_002;
const STRUCTURE_TYPE_DRM_FORMAT_MODIFIER_PROPERTIES_LIST_EXT: i32 = 1_000_158_000;
const STRUCTURE_TYPE_PHYSICAL_DEVICE_DRM_PROPERTIES_EXT: i32 = 1_000_353_000;
const FORMAT_R8G8B8A8_UNORM: i32 = 37;
const FORMAT_B8G8R8A8_UNORM: i32 = 44;
const FORMAT_FEATURE_SAMPLED_IMAGE: u32 = 1;
const MODIFIER_LINEAR: u64 = 0;
const SUCCESS: i32 = 0;

const FORMATS: [(&[u8; 4], i32); 4] = [
    (b"AR24", FORMAT_B8G8R8A8_UNORM),
    (b"XR24", FORMAT_B8G8R8A8_UNORM),
    (b"AB24", FORMAT_R8G8B8A8_UNORM),
    (b"XB24", FORMAT_R8G8B8A8_UNORM),
];

#[repr(C)]
struct ApplicationInfo {
    kind: i32,
    next: *const c_void,
    application_name: *const c_char,
    application_version: u32,
    engine_name: *const c_char,
    engine_version: u32,
    api_version: u32,
}

#[repr(C)]
struct InstanceCreateInfo {
    kind: i32,
    next: *const c_void,
    flags: u32,
    application_info: *const ApplicationInfo,
    enabled_layer_count: u32,
    enabled_layer_names: *const *const c_char,
    enabled_extension_count: u32,
    enabled_extension_names: *const *const c_char,
}

#[repr(C)]
struct DrmProperties {
    kind: i32,
    next: *mut c_void,
    has_primary: u32,
    has_render: u32,
    primary_major: i64,
    primary_minor: i64,
    render_major: i64,
    render_minor: i64,
}

#[repr(C)]
struct DeviceProperties2 {
    kind: i32,
    next: *mut c_void,
    properties: [u64; 103],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ModifierProperties {
    modifier: u64,
    plane_count: u32,
    tiling_features: u32,
}

#[repr(C)]
struct ModifierPropertiesList {
    kind: i32,
    next: *mut c_void,
    count: u32,
    properties: *mut ModifierProperties,
}

#[repr(C)]
struct FormatProperties2 {
    kind: i32,
    next: *mut c_void,
    linear_tiling_features: u32,
    optimal_tiling_features: u32,
    buffer_features: u32,
}

#[link(name = "vulkan")]
unsafe extern "C" {
    fn vkCreateInstance(
        info: *const InstanceCreateInfo,
        allocator: *const c_void,
        instance: *mut *mut c_void,
    ) -> i32;
    fn vkDestroyInstance(instance: *mut c_void, allocator: *const c_void);
    fn vkEnumeratePhysicalDevices(
        instance: *mut c_void,
        count: *mut u32,
        devices: *mut *mut c_void,
    ) -> i32;
    fn vkGetPhysicalDeviceProperties2(device: *mut c_void, properties: *mut DeviceProperties2);
    fn vkGetPhysicalDeviceFormatProperties2(
        device: *mut c_void,
        format: i32,
        properties: *mut FormatProperties2,
    );
}

thread_local! {
    static SAMPLED: OnceCell<Vec<(u32, u64)>> = const { OnceCell::new() };
}

pub fn sampled_modifiers(render: (u64, u64)) -> Vec<(u32, u64)> {
    SAMPLED.with(|sampled| sampled.get_or_init(|| query(render)).clone())
}

fn query((major, minor): (u64, u64)) -> Vec<(u32, u64)> {
    let application = ApplicationInfo {
        kind: STRUCTURE_TYPE_APPLICATION_INFO,
        next: null(),
        application_name: c"proscenio".as_ptr(),
        application_version: 0,
        engine_name: null(),
        engine_version: 0,
        api_version: API_VERSION_1_1,
    };
    let info = InstanceCreateInfo {
        kind: STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
        next: null(),
        flags: 0,
        application_info: &application,
        enabled_layer_count: 0,
        enabled_layer_names: null(),
        enabled_extension_count: 0,
        enabled_extension_names: null(),
    };
    let mut instance = null_mut();
    if unsafe { vkCreateInstance(&info, null(), &mut instance) } != SUCCESS {
        return Vec::new();
    }
    let modifiers = device(instance, major, minor)
        .map(|device| modifiers(device))
        .unwrap_or_default();
    unsafe { vkDestroyInstance(instance, null()) };
    modifiers
}

fn device(instance: *mut c_void, major: u64, minor: u64) -> Option<*mut c_void> {
    let mut count = 0;
    unsafe { vkEnumeratePhysicalDevices(instance, &mut count, null_mut()) };
    let mut devices = vec![null_mut(); count as usize];
    if unsafe { vkEnumeratePhysicalDevices(instance, &mut count, devices.as_mut_ptr()) } != SUCCESS
    {
        return None;
    }
    devices.truncate(count as usize);
    devices.into_iter().find(|device| {
        let mut drm = DrmProperties {
            kind: STRUCTURE_TYPE_PHYSICAL_DEVICE_DRM_PROPERTIES_EXT,
            next: null_mut(),
            has_primary: 0,
            has_render: 0,
            primary_major: 0,
            primary_minor: 0,
            render_major: 0,
            render_minor: 0,
        };
        let mut properties = DeviceProperties2 {
            kind: STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2,
            next: (&mut drm as *mut DrmProperties).cast(),
            properties: [0; 103],
        };
        unsafe { vkGetPhysicalDeviceProperties2(*device, &mut properties) };
        drm.has_render != 0 && drm.render_major as u64 == major && drm.render_minor as u64 == minor
    })
}

fn modifiers(device: *mut c_void) -> Vec<(u32, u64)> {
    let mut found = Vec::new();
    for (fourcc, format) in FORMATS {
        let fourcc = u32::from_le_bytes(*fourcc);
        let mut list = ModifierPropertiesList {
            kind: STRUCTURE_TYPE_DRM_FORMAT_MODIFIER_PROPERTIES_LIST_EXT,
            next: null_mut(),
            count: 0,
            properties: null_mut(),
        };
        let mut properties = FormatProperties2 {
            kind: STRUCTURE_TYPE_FORMAT_PROPERTIES_2,
            next: (&mut list as *mut ModifierPropertiesList).cast(),
            linear_tiling_features: 0,
            optimal_tiling_features: 0,
            buffer_features: 0,
        };
        unsafe { vkGetPhysicalDeviceFormatProperties2(device, format, &mut properties) };
        let mut entries = vec![
            ModifierProperties {
                modifier: 0,
                plane_count: 0,
                tiling_features: 0,
            };
            list.count as usize
        ];
        list.properties = entries.as_mut_ptr();
        properties.next = (&mut list as *mut ModifierPropertiesList).cast();
        unsafe { vkGetPhysicalDeviceFormatProperties2(device, format, &mut properties) };
        entries.truncate(list.count as usize);
        found.extend(
            entries
                .iter()
                .filter(|entry| {
                    entry.modifier != MODIFIER_LINEAR
                        && entry.plane_count == 1
                        && entry.tiling_features & FORMAT_FEATURE_SAMPLED_IMAGE != 0
                })
                .map(|entry| (fourcc, entry.modifier)),
        );
    }
    found
}
