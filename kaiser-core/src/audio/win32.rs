#![cfg(target_os = "windows")]

use std::ffi::c_void;

use serde::{Deserialize, Serialize};
use windows::core::{IUnknown, Interface, GUID, PCWSTR};
use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_Device_FriendlyName, PKEY_DeviceInterface_FriendlyName,
};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eRender, EDataFlow, ERole,
    IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE, DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED, DEVICE_STATE_UNPLUGGED,
};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub flow: AudioFlow,
    pub enabled: bool,
    pub volume: f32,
    pub muted: bool,
    pub is_default_console: bool,
    pub is_default_comms: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFlow {
    Render,
    Capture,
}

pub struct AudioManager {
    _init: ComInitGuard,
}

struct ComInitGuard;

impl ComInitGuard {
    fn new() -> Self {
        unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
        Self
    }
}

impl Drop for ComInitGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

impl AudioManager {
    pub fn new() -> Self {
        Self { _init: ComInitGuard::new() }
    }

    pub fn list_devices(&self) -> anyhow::Result<Vec<AudioDevice>> {
        let enumerator = create_enumerator()?;
        let mut devices = Vec::new();

        let def_render = default_device_id(&enumerator, eRender, eConsole);
        let def_render_comms = default_device_id(&enumerator, eRender, eCommunications);
        let def_capture = default_device_id(&enumerator, eCapture, eConsole);
        let def_capture_comms = default_device_id(&enumerator, eCapture, eCommunications);

        for flow in [eRender, eCapture] {
            let collection = unsafe {
                enumerator
                    .EnumAudioEndpoints(
                        flow,
                        DEVICE_STATE(DEVICE_STATE_ACTIVE.0 | DEVICE_STATE_DISABLED.0 | DEVICE_STATE_UNPLUGGED.0),
                    )
                    .map_err(|e| anyhow::anyhow!("EnumAudioEndpoints: {e}"))?
            };
            let count = unsafe { collection.GetCount()? };
            for i in 0..count {
                let device = unsafe { collection.Item(i)? };
                if let Ok(info) = read_device_info(
                    &device,
                    flow_to_kaiser(flow),
                    &def_render,
                    &def_render_comms,
                    &def_capture,
                    &def_capture_comms,
                ) {
                    devices.push(info);
                }
            }
        }
        Ok(devices)
    }

    pub fn set_volume(&self, device_id: &str, volume: f32) -> anyhow::Result<()> {
        let endpoint = device_endpoint_volume(device_id)?;
        unsafe {
            endpoint.SetMasterVolumeLevelScalar(volume.clamp(0.0, 1.0), &GUID::zeroed())?;
        }
        Ok(())
    }

    pub fn set_mute(&self, device_id: &str, muted: bool) -> anyhow::Result<()> {
        let endpoint = device_endpoint_volume(device_id)?;
        unsafe { endpoint.SetMute(muted, &GUID::zeroed())? };
        Ok(())
    }

    pub fn set_default(&self, device_id: &str) -> anyhow::Result<()> {
        let id_wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        let policy = PolicyConfig::new()
            .map_err(|e| anyhow::anyhow!("PolicyConfig unavailable: {e}"))?;
        // roles: eConsole=0, eMultimedia=1, eCommunications=2
        for role in 0i32..=2 {
            unsafe { policy.set_default_endpoint(&id_wide, role); }
        }
        Ok(())
    }
}

fn create_enumerator() -> anyhow::Result<IMMDeviceEnumerator> {
    unsafe {
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| anyhow::anyhow!("CoCreateInstance IMMDeviceEnumerator: {e}"))
    }
}

fn default_device_id(enumerator: &IMMDeviceEnumerator, flow: EDataFlow, role: ERole) -> Option<String> {
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(flow, role).ok()? };
    get_device_id(&device)
}

fn get_device_id(device: &IMMDevice) -> Option<String> {
    unsafe {
        let pwstr = device.GetId().ok()?;
        let name = pwstr.to_string().ok();
        CoTaskMemFree(Some(pwstr.0.cast()));
        name
    }
}

fn read_device_info(
    device: &IMMDevice,
    flow: AudioFlow,
    def_render: &Option<String>,
    def_render_comms: &Option<String>,
    def_capture: &Option<String>,
    def_capture_comms: &Option<String>,
) -> anyhow::Result<AudioDevice> {
    let id = get_device_id(device).ok_or_else(|| anyhow::anyhow!("no device id"))?;
    let state = unsafe { device.GetState()? };
    let enabled = state == DEVICE_STATE_ACTIVE;
    let name = device_friendly_name(device).unwrap_or_else(|| id.clone());

    let (volume, muted) = if enabled {
        device_endpoint_volume(&id)
            .map(|ep| unsafe {
                let vol = ep.GetMasterVolumeLevelScalar().unwrap_or(0.0);
                let mute = ep.GetMute().unwrap_or_default().as_bool();
                (vol, mute)
            })
            .unwrap_or((0.0, false))
    } else {
        (0.0, false)
    };

    let (def_console, def_comms) = match flow {
        AudioFlow::Render => (
            def_render.as_deref() == Some(&id),
            def_render_comms.as_deref() == Some(&id),
        ),
        AudioFlow::Capture => (
            def_capture.as_deref() == Some(&id),
            def_capture_comms.as_deref() == Some(&id),
        ),
    };

    Ok(AudioDevice { id, name, flow, enabled, volume, muted, is_default_console: def_console, is_default_comms: def_comms })
}

fn device_friendly_name(device: &IMMDevice) -> Option<String> {
    unsafe {
        let store: IPropertyStore = device.OpenPropertyStore(STGM_READ).ok()?;
        let prop = store.GetValue(&PKEY_Device_FriendlyName)
            .or_else(|_| store.GetValue(&PKEY_DeviceInterface_FriendlyName))
            .ok()?;
        let pwstr = PropVariantToStringAlloc(&prop).ok()?;
        let name = pwstr.to_string().ok();
        CoTaskMemFree(Some(pwstr.0.cast()));
        name
    }
}

fn device_endpoint_volume(device_id: &str) -> anyhow::Result<IAudioEndpointVolume> {
    let enumerator = create_enumerator()?;
    let id_wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
    let device = unsafe {
        enumerator
            .GetDevice(PCWSTR(id_wide.as_ptr()))
            .map_err(|e| anyhow::anyhow!("GetDevice: {e}"))?
    };
    unsafe {
        device
            .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            .map_err(|e| anyhow::anyhow!("Activate IAudioEndpointVolume: {e}"))
    }
}

fn flow_to_kaiser(flow: EDataFlow) -> AudioFlow {
    if flow == eCapture { AudioFlow::Capture } else { AudioFlow::Render }
}

// ---------------------------------------------------------------------------
// IPolicyConfig — undocumented COM interface for setting default audio endpoint.
//
// Vtable layout (Windows 10/11):
//   [0]  QueryInterface   \ IUnknown
//   [1]  AddRef           |
//   [2]  Release          /
//   [3]  GetMixFormat     \
//   [4]  GetDeviceFormat   |
//   [5]  ResetDeviceFormat |
//   [6]  SetDeviceFormat   |  IPolicyConfig (before SetDefaultEndpoint)
//   [7]  GetProcessingPeriod|
//   [8]  SetProcessingPeriod|
//   [9]  GetShareMode     |
//   [10] SetShareMode     |
//   [11] GetPropertyValue |
//   [12] SetPropertyValue /
//   [13] SetDefaultEndpoint  <-- we need this slot
//   [14] SetEndpointVisibility
// ---------------------------------------------------------------------------

const CLSID_POLICY_CONFIG: GUID = GUID::from_values(
    0x870af99c, 0x171d, 0x4f9e,
    [0xaf, 0x0d, 0xe6, 0x3d, 0xf4, 0x0c, 0x2b, 0xc9],
);

const IID_POLICY_CONFIG: GUID = GUID::from_values(
    0x6be54be8, 0xa068, 0x4875,
    [0xa4, 0x9d, 0x0c, 0x29, 0x66, 0x47, 0x3b, 0x11],
);

// Raw vtable: 3 IUnknown + 10 IPolicyConfig pads + SetDefaultEndpoint
#[repr(C)]
struct RawPolicyConfigVtbl {
    _iunknown: [usize; 3],
    _pad: [usize; 10],
    set_default_endpoint:
        unsafe extern "system" fn(this: *mut c_void, device_id: *const u16, role: i32) -> i32,
}

struct PolicyConfig(*mut c_void);

unsafe impl Send for PolicyConfig {}

impl PolicyConfig {
    fn new() -> anyhow::Result<Self> {
        unsafe {
            let unk: IUnknown = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)
                .map_err(|e| anyhow::anyhow!("CoCreateInstance PolicyConfig: {e}"))?;

            let unk_raw = unk.as_raw();
            // vtable pointer is the first field in every COM object
            let vtbl = *(unk_raw as *const *const usize);
            // QueryInterface is always slot 0 in the vtable
            let qi: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32 =
                std::mem::transmute(*vtbl);
            let mut policy_raw: *mut c_void = std::ptr::null_mut();
            let hr = qi(unk_raw, &IID_POLICY_CONFIG, &mut policy_raw);

            drop(unk); // Release IUnknown ref; QI already AddRef'd policy_raw

            if hr != 0 || policy_raw.is_null() {
                return Err(anyhow::anyhow!(
                    "QueryInterface for IPolicyConfig failed: hr={hr:#x}"
                ));
            }
            Ok(Self(policy_raw))
        }
    }

    unsafe fn set_default_endpoint(&self, device_id_wide: &[u16], role: i32) {
        let vtbl = *(self.0 as *const *const RawPolicyConfigVtbl);
        ((*vtbl).set_default_endpoint)(self.0, device_id_wide.as_ptr(), role);
    }
}

impl Drop for PolicyConfig {
    fn drop(&mut self) {
        unsafe {
            let vtbl = *(self.0 as *const *const RawPolicyConfigVtbl);
            let release: unsafe extern "system" fn(*mut c_void) -> u32 =
                std::mem::transmute((*vtbl)._iunknown[2]);
            release(self.0);
        }
    }
}
