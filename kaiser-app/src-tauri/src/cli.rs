use kaiser_core::{AudioManager, KaiserBackend, KaiserConfigStore};
use monarch::MonarchDisplayManager;

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let store_path = KaiserConfigStore::default_path();
    let store = KaiserConfigStore::new(store_path.clone());

    match args {
        [flag, name] if flag == "--apply-profile" => {
            cmd_apply_profile(&store, name)?;
        }
        [flag] if flag == "--list-profiles" => {
            cmd_list_profiles(&store);
        }
        [flag] if flag == "--list-audio-devices" => {
            cmd_list_audio_devices()?;
        }
        [flag] if flag == "--list-displays" => {
            cmd_list_displays(&store)?;
        }
        [flag, device_id, volume] if flag == "--set-volume" => {
            let vol: f32 = volume.parse().map_err(|_| anyhow::anyhow!("invalid volume (0.0–1.0)"))?;
            cmd_set_volume(device_id, vol)?;
        }
        [flag, device_id, value] if flag == "--set-mute" => {
            let muted: bool = value.parse().map_err(|_| anyhow::anyhow!("invalid mute value (true/false)"))?;
            cmd_set_mute(device_id, muted)?;
        }
        [flag, device_id] if flag == "--set-default-audio" => {
            cmd_set_default_audio(device_id)?;
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print_help();
        }
        other => {
            eprintln!("unknown arguments: {:?}", other);
            print_help();
            return Err(anyhow::anyhow!("unknown arguments"));
        }
    }

    Ok(())
}

fn cmd_apply_profile(store: &KaiserConfigStore, name: &str) -> anyhow::Result<()> {
    let backend = KaiserBackend::new();
    let store_for_manager = KaiserConfigStore::new(KaiserConfigStore::default_path());
    let mut manager = MonarchDisplayManager::new(backend, store_for_manager)
        .map_err(|e| anyhow::anyhow!("init display manager: {e}"))?;

    manager.apply_profile(name)
        .map_err(|e| anyhow::anyhow!("apply profile '{name}': {e}"))?;
    manager.confirm_current_layout()
        .map_err(|e| anyhow::anyhow!("confirm layout: {e}"))?;

    println!("applied display profile: {name}");

    // Apply audio settings
    if let Some(kp) = store.load_kaiser_profile(name) {
        if !kp.audio.is_empty() {
            let audio = AudioManager::new();
            if let Ok(devices) = audio.list_devices() {
                for setting in &kp.audio {
                    let matches: Vec<_> = devices
                        .iter()
                        .filter(|d| d.name.to_lowercase().contains(&setting.pattern.to_lowercase()))
                        .collect();
                    for device in matches {
                        if let Some(vol) = setting.volume {
                            let _ = audio.set_volume(&device.id, vol);
                        }
                        if let Some(muted) = setting.muted {
                            let _ = audio.set_mute(&device.id, muted);
                        }
                        if setting.set_default == Some(true) {
                            let _ = audio.set_default(&device.id);
                            println!("set default audio: {}", device.name);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn cmd_list_profiles(store: &KaiserConfigStore) {
    let names = store.list_profile_names();
    if names.is_empty() {
        println!("(no profiles saved)");
        return;
    }
    for name in names {
        println!("{name}");
    }
}

fn cmd_list_audio_devices() -> anyhow::Result<()> {
    let audio = AudioManager::new();
    let devices = audio.list_devices().map_err(|e| anyhow::anyhow!("list audio devices: {e}"))?;
    for d in devices {
        println!(
            "[{}] {} | flow={:?} enabled={} vol={:.0}% muted={} default_console={}",
            d.id,
            d.name,
            d.flow,
            d.enabled,
            d.volume * 100.0,
            d.muted,
            d.is_default_console,
        );
    }
    Ok(())
}

fn cmd_list_displays(_store: &KaiserConfigStore) -> anyhow::Result<()> {
    use monarch::DisplayBackend;
    let backend = KaiserBackend::new();
    let displays = backend.list_displays()
        .map_err(|e| anyhow::anyhow!("list displays: {e}"))?;
    for d in displays {
        println!(
            "{} [{}:{}] {}x{} @{}Hz active={} primary={}",
            d.friendly_name,
            d.id.adapter_luid,
            d.id.target_id,
            d.resolution.width,
            d.resolution.height,
            d.refresh_rate_mhz / 1000,
            d.is_active,
            d.is_primary,
        );
    }
    Ok(())
}

fn cmd_set_volume(device_id: &str, volume: f32) -> anyhow::Result<()> {
    let audio = AudioManager::new();
    audio.set_volume(device_id, volume)
        .map_err(|e| anyhow::anyhow!("set volume: {e}"))?;
    println!("set volume to {:.0}% for {device_id}", volume * 100.0);
    Ok(())
}

fn cmd_set_mute(device_id: &str, muted: bool) -> anyhow::Result<()> {
    let audio = AudioManager::new();
    audio.set_mute(device_id, muted)
        .map_err(|e| anyhow::anyhow!("set mute: {e}"))?;
    println!("{} {device_id}", if muted { "muted" } else { "unmuted" });
    Ok(())
}

fn cmd_set_default_audio(device_id: &str) -> anyhow::Result<()> {
    let audio = AudioManager::new();
    audio.set_default(device_id)
        .map_err(|e| anyhow::anyhow!("set default audio: {e}"))?;
    println!("set default audio device: {device_id}");
    Ok(())
}

fn print_help() {
    println!(
        r#"Kaiser - Display & Audio Profile Manager

Usage:
  kaiser.exe                                     Open GUI
  kaiser.exe --apply-profile <name>              Apply a saved profile
  kaiser.exe --list-profiles                     List saved profiles
  kaiser.exe --list-displays                     List connected displays
  kaiser.exe --list-audio-devices                List audio devices
  kaiser.exe --set-volume <device-id> <0.0-1.0>  Set device volume
  kaiser.exe --set-mute <device-id> <true|false> Mute/unmute device
  kaiser.exe --set-default-audio <device-id>     Set default audio device
  kaiser.exe --help                              Show this help
"#
    );
}
