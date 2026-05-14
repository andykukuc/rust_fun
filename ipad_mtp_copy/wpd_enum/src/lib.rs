use windows::{
    core::*,
    Win32::Devices::PortableDevices::*,
    Win32::System::Com::*,
};

pub fn list_dcim() -> windows::core::Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;

        let manager: IPortableDeviceManager =
            CoCreateInstance(&PortableDeviceManager, None, CLSCTX_INPROC_SERVER)?;

        // Get device count
        let mut count = 0u32;
        manager.GetDevices(std::ptr::null_mut(), &mut count)?;
        if count == 0 {
            println!("No WPD devices found");
            CoUninitialize();
            return Ok(());
        }

        // Get device IDs
        let mut ids: Vec<PWSTR> = vec![PWSTR::null(); count as usize];
        manager.GetDevices(ids.as_mut_ptr(), &mut count)?;

        println!("=== COM-based WPD enumeration ===");
        for id in ids {
            if id.is_null() {
                continue;
            }

            // Convert PWSTR -> HSTRING (device id)
            let mut len = 0usize;
            while *id.0.add(len) != 0 {
                len += 1;
            }
            if len == 0 {
                continue;
            }
            let devid = HSTRING::from_wide(std::slice::from_raw_parts(id.0, len));

            // Friendly name
            let mut name_len = 0u32;
            manager.GetDeviceFriendlyName(&devid, PWSTR::null(), &mut name_len)?;
            let mut name_buf: Vec<u16> = vec![0; name_len as usize];
            manager.GetDeviceFriendlyName(&devid, PWSTR(name_buf.as_mut_ptr()), &mut name_len)?;
            let friendly_name = String::from_utf16_lossy(
                &name_buf[..(name_len as usize).saturating_sub(1)]
            );
            println!("Device: {}", friendly_name);

            // Open device
            let device: IPortableDevice =
                CoCreateInstance(&PortableDeviceFTM, None, CLSCTX_INPROC_SERVER)?;
            let client_info: IPortableDeviceValues =
                CoCreateInstance(&PortableDeviceValues, None, CLSCTX_INPROC_SERVER)?;
            client_info.SetStringValue(&WPD_CLIENT_NAME, &HSTRING::from("Rust WPD Client"))?;
            client_info.SetUnsignedIntegerValue(&WPD_CLIENT_MAJOR_VERSION, 1)?;
            client_info.SetUnsignedIntegerValue(&WPD_CLIENT_MINOR_VERSION, 0)?;
            client_info.SetUnsignedIntegerValue(&WPD_CLIENT_REVISION, 0)?;
            if let Err(e) = device.Open(&devid, &client_info) {
                println!("⚠ Failed to open device: {e}");
                continue;
            }

            let content = match device.Content() {
                Ok(c) => c,
                Err(e) => {
                    println!("⚠ Failed to get content: {e}");
                    continue;
                }
            };

            // Enum root ("DEVICE")
            let device_id_utf16: Vec<u16> = "DEVICE\0".encode_utf16().collect();
            let enum_objects = match content.EnumObjects(
                0,
                PCWSTR(device_id_utf16.as_ptr()),
                None
            ) {
                Ok(e) => e,
                Err(e) if e.code() == HRESULT(0x80070490u32 as i32) => {
                    println!("(Root is empty)");
                    continue;
                }
                Err(e) => {
                    println!("⚠ Failed to enumerate root: {e}");
                    continue;
                }
            };

            // Find "Internal Storage"
            let mut internal_storage_id: Option<Vec<u16>> = None;
            loop {
                let mut obj_ids: [PWSTR; 16] = [PWSTR::null(); 16];
                let mut fetched: u32 = 0;
                let hr = enum_objects.Next(&mut obj_ids, &mut fetched);

                if hr.is_ok() {
                    if fetched == 0 {
                        break;
                    }
                    for obj_id in &obj_ids[..fetched as usize] {
                        if obj_id.is_null() {
                            continue;
                        }

                        // copy object ID immediately (owned UTF-16, no null terminator)
                        let mut id_len = 0usize;
                        while *obj_id.0.add(id_len) != 0 { id_len += 1; }
                        let obj_id_owned: Vec<u16> =
                            std::slice::from_raw_parts(obj_id.0, id_len).to_vec();

                        // get properties/values while obj_id pointer is valid,
                        // then copy the object name into an owned Rust String immediately
                        let properties = match content.Properties() {
                            Ok(p) => p,
                            Err(e) => {
                                println!("⚠ Failed to get properties: {e}");
                                continue;
                            }
                        };
                        let values = match properties.GetValues(PCWSTR(obj_id.0), None) {
                            Ok(v) => v,
                            Err(e) => {
                                println!("⚠ Failed to GetValues: {e}");
                                continue;
                            }
                        };
                        let name_result = values.GetStringValue(&WPD_OBJECT_NAME);
                        let name: String = match name_result {
                            Ok(name_pwstr) => {
                                let mut nlen = 0usize;
                                while *name_pwstr.0.add(nlen) != 0 { nlen += 1; }
                                String::from_utf16_lossy(std::slice::from_raw_parts(name_pwstr.0, nlen))
                            }
                            Err(e) if e.code() == HRESULT(0x80070490u32 as i32) => {
                                "(no name)".to_string()
                            }
                            Err(e) => {
                                println!("⚠ Failed to GetStringValue: {e}");
                                "(error)".to_string()
                            }
                        };

                        println!("  Object: {}  (ID len {})", name, obj_id_owned.len());

                        if name == "Internal Storage" {
                            // store a null-terminated copy for later PCWSTR use
                            let mut id_copy: Vec<u16> = obj_id_owned.clone();
                            id_copy.push(0);
                            internal_storage_id = Some(id_copy);
                        }
                    }
                } else if hr == HRESULT(0x80070490u32 as i32) {
                    println!("(Root enumeration ended: Element not found)");
                    break;
                } else {
                    println!("⚠ Root enumeration failed: {hr:?}");
                    break;
                }
            }

            // Drill into Internal Storage (if found)
            if let Some(storage_id) = internal_storage_id {
                let enum_storage = match content.EnumObjects(
                    0,
                    PCWSTR(storage_id.as_ptr()),
                    None
                ) {
                    Ok(e) => e,
                    Err(e) if e.code() == HRESULT(0x80070490u32 as i32) => {
                        println!("(Internal Storage is empty or DCIM not exposed)");
                        // Diagnostic raw dump attempt (best-effort)
                        println!("--- Diagnostic: Raw dump of Internal Storage ---");
                        let raw_hr = content.EnumObjects(0, PCWSTR(storage_id.as_ptr()), None);
                        match raw_hr {
                            Ok(raw_enum) => {
                                let mut any = false;
                                loop {
                                    let mut obj_ids: [PWSTR; 16] = [PWSTR::null(); 16];
                                    let mut fetched: u32 = 0;
                                    let hr2 = raw_enum.Next(&mut obj_ids, &mut fetched);
                                    if !hr2.is_ok() || fetched == 0 {
                                        break;
                                    }
                                    any = true;
                                    for obj_id in &obj_ids[..fetched as usize] {
                                        if obj_id.is_null() { continue; }
                                        // copy id and name like above
                                        let mut id_len = 0usize;
                                        while *obj_id.0.add(id_len) != 0 { id_len += 1; }
                                        let obj_id_owned: Vec<u16> =
                                            std::slice::from_raw_parts(obj_id.0, id_len).to_vec();

                                        let props = match content.Properties() {
                                            Ok(p) => p,
                                            Err(e) => {
                                                println!("    (Failed to get properties: {e})");
                                                continue;
                                            }
                                        };
                                        let vals = match props.GetValues(PCWSTR(obj_id.0), None) {
                                            Ok(v) => v,
                                            Err(e) => {
                                                println!("    (Failed to GetValues: {e})");
                                                continue;
                                            }
                                        };
                                        let name = match vals.GetStringValue(&WPD_OBJECT_NAME) {
                                            Ok(n) => {
                                                let mut nlen = 0usize;
                                                while *n.0.add(nlen) != 0 { nlen += 1; }
                                                String::from_utf16_lossy(std::slice::from_raw_parts(n.0, nlen))
                                            }
                                            Err(e) if e.code() == HRESULT(0x80070490u32 as i32) => "(no name)".to_string(),
                                            Err(e) => {
                                                println!("    (Failed to GetStringValue: {e})");
                                                "(error)".to_string()
                                            }
                                        };
                                        println!("    [RAW] {}  (ID len {})", name, obj_id_owned.len());
                                    }
                                }
                                if !any {
                                    println!("    (No raw children found either)");
                                }
                            }
                            Err(e) => println!("    (Raw dump failed: {e})"),
                        }
                        continue;
                    }
                    Err(e) => {
                        println!("⚠ Failed to enumerate Internal Storage: {e}");
                        continue;
                    }
                };

                // Search for DCIM inside Internal Storage
                let mut dcim_id: Option<Vec<u16>> = None;
                loop {
                    let mut obj_ids: [PWSTR; 16] = [PWSTR::null(); 16];
                    let mut fetched: u32 = 0;
                    let hr = enum_storage.Next(&mut obj_ids, &mut fetched);

                    if hr.is_ok() {
                        if fetched == 0 {
                            break;
                        }
                        for obj_id in &obj_ids[..fetched as usize] {
                            if obj_id.is_null() {
                                continue;
                            }

                            // copy id immediately
                            let mut id_len = 0usize;
                            while *obj_id.0.add(id_len) != 0 { id_len += 1; }
                            let obj_id_owned: Vec<u16> =
                                std::slice::from_raw_parts(obj_id.0, id_len).to_vec();

                            let properties = match content.Properties() {
                                Ok(p) => p,
                                Err(e) => {
                                    println!("⚠ Failed to get properties: {e}");
                                    continue;
                                }
                            };
                            let values = match properties.GetValues(PCWSTR(obj_id.0), None) {
                                Ok(v) => v,
                                Err(e) => {
                                    println!("⚠ Failed to GetValues: {e}");
                                    continue;
                                }
                            };
                            let name: String = match values.GetStringValue(&WPD_OBJECT_NAME) {
                                Ok(name_pwstr) => {
                                    let mut nlen = 0usize;
                                    while *name_pwstr.0.add(nlen) != 0 { nlen += 1; }
                                    String::from_utf16_lossy(std::slice::from_raw_parts(name_pwstr.0, nlen))
                                }
                                Err(e) if e.code() == HRESULT(0x80070490u32 as i32) => "(no name)".to_string(),
                                Err(e) => {
                                    println!("⚠ Failed to GetStringValue: {e}");
                                    "(error)".to_string()
                                }
                            };
                            println!("    Folder: {}", name);

                            if name == "DCIM" {
                                let mut id_copy: Vec<u16> = obj_id_owned.clone();
                                id_copy.push(0);
                                dcim_id = Some(id_copy);
                            }
                        }
                    } else if hr == HRESULT(0x80070490u32 as i32) {
                        println!("(Internal Storage enumeration ended: Element not found)");
                        break;
                    } else {
                        println!("⚠ Internal Storage enumeration failed: {hr:?}");
                        break;
                    }
                }

                // If DCIM found, list files (and provide diagnostic fallback)
                if let Some(dcim) = dcim_id {
                    let enum_dcim = match content.EnumObjects(0, PCWSTR(dcim.as_ptr()), None) {
                        Ok(e) => e,
                        Err(e) if e.code() == HRESULT(0x80070490u32 as i32) => {
                            println!("(DCIM is empty)");
                            // diagnostic raw dump for DCIM
                            println!("--- Diagnostic: Raw dump of DCIM ---");
                            let raw_hr = content.EnumObjects(0, PCWSTR(dcim.as_ptr()), None);
                            match raw_hr {
                                Ok(raw_enum) => {
                                    let mut any = false;
                                    loop {
                                        let mut file_ids: [PWSTR; 16] = [PWSTR::null(); 16];
                                        let mut fetched: u32 = 0;
                                        let hr2 = raw_enum.Next(&mut file_ids, &mut fetched);
                                        if !hr2.is_ok() || fetched == 0 {
                                            break;
                                        }
                                        any = true;
                                        for file_id in &file_ids[..fetched as usize] {
                                            if file_id.is_null() { continue; }
                                            // copy file id and name safely
                                            let mut id_len = 0usize;
                                            while *file_id.0.add(id_len) != 0 { id_len += 1; }
                                            let _file_id_owned: Vec<u16> =
                                                std::slice::from_raw_parts(file_id.0, id_len).to_vec();

                                            let props = match content.Properties() {
                                                Ok(p) => p,
                                                Err(e) => {
                                                    println!("    (Failed to get properties: {e})");
                                                    continue;
                                                }
                                            };
                                            let vals = match props.GetValues(PCWSTR(file_id.0), None) {
                                                Ok(v) => v,
                                                Err(e) => {
                                                    println!("    (Failed to GetValues: {e})");
                                                    continue;
                                                }
                                            };
                                            let name = match vals.GetStringValue(&WPD_OBJECT_NAME) {
                                                Ok(n) => {
                                                    let mut nlen = 0usize;
                                                    while *n.0.add(nlen) != 0 { nlen += 1; }
                                                    String::from_utf16_lossy(std::slice::from_raw_parts(n.0, nlen))
                                                }
                                                Err(e) if e.code() == HRESULT(0x80070490u32 as i32) => "(no name)".to_string(),
                                                Err(e) => {
                                                    println!("    (Failed to GetStringValue: {e})");
                                                    "(error)".to_string()
                                                }
                                            };
                                            println!("    [RAW FILE] {}", name);
                                        }
                                    }
                                    if !any {
                                        println!("    (No raw files found in DCIM either)");
                                    }
                                }
                                Err(e) => println!("    (Raw DCIM dump failed: {e})"),
                            }
                            continue;
                        }
                        Err(e) => {
                            println!("⚠ Failed to enumerate DCIM: {e}");
                            continue;
                        }
                    };

                    // list actual files
                    loop {
                        let mut file_ids: [PWSTR; 16] = [PWSTR::null(); 16];
                        let mut fetched: u32 = 0;
                        let hr = enum_dcim.Next(&mut file_ids, &mut fetched);

                        if hr.is_ok() {
                            if fetched == 0 {
                                break;
                            }
                            for file_id in &file_ids[..fetched as usize] {
                                if file_id.is_null() {
                                    continue;
                                }

                                // copy file id
                                let mut id_len = 0usize;
                                while *file_id.0.add(id_len) != 0 { id_len += 1; }
                                let _file_id_owned: Vec<u16> =
                                    std::slice::from_raw_parts(file_id.0, id_len).to_vec();

                                let properties = match content.Properties() {
                                    Ok(p) => p,
                                    Err(e) => {
                                        println!("⚠ Failed to get properties: {e}");
                                        continue;
                                    }
                                };
                                let values = match properties.GetValues(PCWSTR(file_id.0), None) {
                                    Ok(v) => v,
                                    Err(e) => {
                                        println!("⚠ Failed to GetValues: {e}");
                                        continue;
                                    }
                                };
                                let name: String = match values.GetStringValue(&WPD_OBJECT_NAME) {
                                    Ok(name_pwstr) => {
                                        let mut nlen = 0usize;
                                        while *name_pwstr.0.add(nlen) != 0 { nlen += 1; }
                                        String::from_utf16_lossy(std::slice::from_raw_parts(name_pwstr.0, nlen))
                                    }
                                    Err(e) if e.code() == HRESULT(0x80070490u32 as i32) => "(no name)".to_string(),
                                    Err(e) => {
                                        println!("⚠ Failed to GetStringValue: {e}");
                                        "(error)".to_string()
                                    }
                                };
                                println!("      File: {}", name);
                            }
                        } else if hr == HRESULT(0x80070490u32 as i32) {
                            println!("(DCIM enumeration ended: Element not found)");
                            break;
                        } else {
                            println!("⚠ DCIM enumeration failed: {hr:?}");
                            break;
                        }
                    }
                } else {
                    println!("(DCIM not found under Internal Storage)");
                }
            } else {
                println!("(Internal Storage not found)");
            }
        }

        CoUninitialize();
    }
    Ok(())
}
