#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::os::windows::process::CommandExt;
use winreg::enums::*;
use winreg::RegKey;

const CURRENT_VERSION: &str = "1.2.0";   // ← Update this when you release new version

#[derive(Clone, Debug)]
pub struct NetworkAdapterInfo {
    pub id: String,
    pub description: String,
    pub name: String,
    pub interface_name: String,
}

impl NetworkAdapterInfo {
    pub fn display_name(&self) -> String {
        format!("{} ({})", self.name, self.description)
    }
}

pub fn is_admin() -> bool {
    is_elevated::is_elevated()
}

fn check_for_update() {
    match reqwest::blocking::get("https://raw.githubusercontent.com/PshNsDev/RemoveBanApiAsync/refs/heads/main/version.txt") {
        Ok(response) => {
            if let Ok(latest) = response.text() {
                let latest = latest.trim();
                if latest != CURRENT_VERSION {
                    let msg = format!("New version available ({}).\nCurrent version: {}\n\nDo you want to download it?", latest, CURRENT_VERSION);
                    let result = Command::new("powershell")
                        .args(&["-Command", &format!("Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('{}', 'Update Available', 'YesNo', 'Question')", msg.replace("'", "`'"))])
                        .creation_flags(0x08000000)
                        .output();

                    if let Ok(out) = result {
                        if String::from_utf8_lossy(&out.stdout).trim() == "Yes" {
                            let _ = Command::new("cmd").args(&["/c", "start", "https://github.com/PshNsDev/RemoveBanApiAsync/releases"]).spawn();
                        }
                    }
                }
            }
        }
        Err(_) => {
            // No internet
            let _ = Command::new("powershell")
                .args(&["-Command", "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('No wifi detected.', 'Connection Error', 'OK', 'Warning')"])
                .creation_flags(0x08000000)
                .output();
        }
    }
}

pub fn delete_roblox_cookies() -> Result<String, String> {
    if let Some(local_app_data) = dirs::data_local_dir() {
        let cookie_path: PathBuf = local_app_data
            .join("Roblox")
            .join("LocalStorage")
            .join("RobloxCookies.dat");

        if cookie_path.exists() {
            fs::remove_file(&cookie_path).map(|_| "[√] Roblox cookie deleted successfully.".to_string())
                .map_err(|e| format!("[!!!] Error deleting cookie: {}", e))
        } else {
            Ok("[!] Cookie file not found.".to_string())
        }
    } else {
        Err("[!] Could not locate LocalAppData folder.".to_string())
    }
}

pub fn get_network_adapters() -> Vec<NetworkAdapterInfo> {
    let mut adapters = Vec::new();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let path = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}";

    if let Ok(class_key) = hklm.open_subkey(path) {
        for subkey_name in class_key.enum_keys().filter_map(|x| x.ok()) {
            if subkey_name.len() == 4 {
                if let Ok(key) = class_key.open_subkey(&subkey_name) {
                    let driver_desc: String = key.get_value("DriverDesc").unwrap_or_default();
                    let net_cfg_id: String = key.get_value("NetCfgInstanceID").unwrap_or_default();
                    let net_connection_id: String = key.get_value("NetConnectionID").unwrap_or_default();

                    if !driver_desc.is_empty() && !net_cfg_id.is_empty() {
                        let interface_name = if !net_connection_id.is_empty() {
                            net_connection_id
                        } else {
                            get_adapter_interface_name(&net_cfg_id).unwrap_or_else(|_| driver_desc.clone())
                        };

                        adapters.push(NetworkAdapterInfo {
                            id: subkey_name,
                            description: driver_desc.clone(),
                            name: driver_desc.clone(),
                            interface_name,
                        });
                    }
                }
            }
        }
    }
    adapters
}

fn get_adapter_interface_name(net_cfg_id: &str) -> Result<String, String> {
    let script = format!("Get-NetAdapter | Where-Object {{ $_.InterfaceGuid -eq '{}' }} | Select-Object -ExpandProperty Name", net_cfg_id);
    let output = Command::new("powershell").args(&["-Command", &script]).stdout(Stdio::piped()).creation_flags(0x08000000).output().map_err(|e| e.to_string())?;
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !name.is_empty() { Ok(name) } else { Err("Failed".to_string()) }
}

pub fn spoof_mac(adapter_id: &str, new_mac: &str) -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let subkey_path = format!(r"SYSTEM\CurrentControlSet\Control\Class\{{4d36e972-e325-11ce-bfc1-08002be10318}}\{}", adapter_id);
    let key = hklm.open_subkey_with_flags(&subkey_path, KEY_WRITE).map_err(|e| format!("Error opening registry: {}", e))?;
    key.set_value("NetworkAddress", &new_mac).map_err(|e| format!("Error setting MAC: {}", e))?;
    Ok(())
}

pub fn restart_adapter(interface_name: &str) -> Result<(), String> {
    let ps_script = format!("Restart-NetAdapter -Name '{}' -Confirm:$false", interface_name);
    let output = Command::new("powershell").args(&["-Command", &ps_script]).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(0x08000000).output().map_err(|e| format!("Failed to execute PowerShell: {}", e))?;
    if output.status.success() { Ok(()) } else {
        Err(format!("Restart error: {}", String::from_utf8_lossy(&output.stderr).trim()))
    }
}

fn show_error_in_cmd(msg: &str) {
    let _ = Command::new("cmd").args(&["/c", "echo", msg, "&", "pause"]).spawn();
}

struct AppGui {
    adapters: Vec<NetworkAdapterInfo>,
    selected_index: usize,
    enable_spoof: bool,
    is_admin_user: bool,
    header_texture: Option<egui::TextureHandle>,
}

impl AppGui {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            adapters: Vec::new(),
            selected_index: 0,
            enable_spoof: false,
            is_admin_user: is_admin(),
            header_texture: None,
        };

        let icon_bytes = include_bytes!("../icon.ico");
        if let Ok(img) = image::load_from_memory(icon_bytes) {
            let img_rgba = img.to_rgba8();
            let size = [img_rgba.width() as _, img_rgba.height() as _];
            let pixels = img_rgba.into_raw();
            app.header_texture = Some(cc.egui_ctx.load_texture("app_icon", egui::ColorImage::from_rgba_unmultiplied(size, &pixels), egui::TextureOptions::default()));
        }

        app.refresh_adapters();
        app
    }

    fn refresh_adapters(&mut self) {
        self.adapters = get_network_adapters();
    }
}

impl eframe::App for AppGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(texture) = &self.header_texture {
                    ui.image((texture.id(), egui::vec2(32.0, 32.0)));
                }
                ui.heading("Remove Ban Api Async v1.2");
            });

            ui.add_space(15.0);

            if !self.is_admin_user {
                ui.label(egui::RichText::new("⚠️ Run as Administrator!").color(egui::Color32::LIGHT_RED).strong());
            }

            ui.group(|ui| {
                ui.label(egui::RichText::new("Cookie Cleanup").strong());
                let btn = ui.button("Delete Roblox Cookie File");
                if btn.clicked() {
                    match delete_roblox_cookies() {
                        Ok(_) => {},
                        Err(err) => show_error_in_cmd(&err),
                    }
                }
                if btn.hovered() { ctx.set_cursor_icon(egui::CursorIcon::PointingHand); }
            });

            ui.add_space(15.0);

            ui.group(|ui| {
                ui.label(egui::RichText::new("MAC Address Spoofing").strong());
                let toggle = ui.checkbox(&mut self.enable_spoof, "Enable MAC Address Change?");
                if toggle.hovered() { ctx.set_cursor_icon(egui::CursorIcon::PointingHand); }

                ui.add_enabled_ui(self.enable_spoof, |ui| {
                    ui.label("Select Network Adapter:");

                    if !self.adapters.is_empty() {
                        let selected_text = self.adapters[self.selected_index].display_name();
                        egui::ComboBox::from_id_source("adapter_combo")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for (i, adapter) in self.adapters.iter().enumerate() {
                                    ui.selectable_value(&mut self.selected_index, i, adapter.display_name());
                                }
                            });
                    } else {
                        ui.label("No adapters found.");
                    }

                    ui.horizontal(|ui| {
                        let refresh_btn = ui.button("Refresh Adapters");
                        if refresh_btn.clicked() { self.refresh_adapters(); }
                        if refresh_btn.hovered() { ctx.set_cursor_icon(egui::CursorIcon::PointingHand); }

                        let spoof_btn = ui.button("Spoof Selected MAC");
                        if spoof_btn.clicked() {
                            if let Some(adapter) = self.adapters.get(self.selected_index).cloned() {
                                let new_mac = generate_valid_laa_mac();
                                match spoof_mac(&adapter.id, &new_mac) {
                                    Ok(_) => {
                                        if let Err(e) = restart_adapter(&adapter.interface_name) {
                                            show_error_in_cmd(&e);
                                        }
                                    }
                                    Err(e) => show_error_in_cmd(&e),
                                }
                            }
                        }
                        if spoof_btn.hovered() { ctx.set_cursor_icon(egui::CursorIcon::PointingHand); }
                    });
                });
            });
        });
    }
}

pub fn generate_valid_laa_mac() -> String {
    let mut bytes: [u8; 6] = rand::random();
    bytes[0] = (bytes[0] & 0xFC) | 0x02;
    bytes.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join("")
}

fn main() -> Result<(), eframe::Error> {
    check_for_update();

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([650.0, 480.0])
        .with_min_inner_size([650.0, 480.0])
        .with_max_inner_size([650.0, 480.0])
        .with_resizable(false)
        .with_title("Remove Ban Api Async v1.2");

    let mut options = eframe::NativeOptions { viewport, ..Default::default() };

    if let Some(icon) = load_app_icon() {
        options.viewport = options.viewport.with_icon(icon);
    }

    eframe::run_native("Remove Ban Api Async", options, Box::new(|cc| Box::new(AppGui::new(cc))))
}

fn load_app_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("../icon.ico");
    if let Ok(image) = image::load_from_memory(icon_bytes) {
        let image = image.to_rgba8();
        let (width, height) = image.dimensions();
        Some(egui::IconData { rgba: image.into_raw(), width, height })
    } else { None }
}