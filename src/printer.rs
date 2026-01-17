use serde::Deserialize;

/// Main printer state aggregated from MQTT messages
#[derive(Debug, Clone, Default)]
pub struct PrinterState {
    pub connected: bool,
    pub printer_name: String,
    pub printer_model: String,
    pub print_status: PrintStatus,
    pub temperatures: Temperatures,
    pub speeds: Speeds,
    pub ams: Option<AmsState>,
    pub lights: LightState,
    pub wifi_signal: i32,
    pub gcode_state: String,
    pub hms_errors: Vec<HmsError>,
}

#[derive(Debug, Clone, Default)]
pub struct PrintStatus {
    pub gcode_file: String,
    pub subtask_name: String,
    pub project_id: String,
    pub task_id: String,
    pub progress: u8,
    pub layer_num: u32,
    pub total_layers: u32,
    pub remaining_time_mins: u32,
    pub gcode_state: String,
    pub print_type: String,
}

impl PrintStatus {
    /// Returns the best available display name for the print job.
    /// For cloud prints with only slicer profile info, shows "Cloud: <profile>".
    /// For local prints, shows the actual filename.
    pub fn display_name(&self) -> String {
        let subtask = self.clean_name(&self.subtask_name);
        let gcode = self.clean_name(&self.gcode_file);

        // If subtask_name looks like an actual name (not slicer settings), use it
        if !subtask.is_empty() && !self.looks_like_slicer_profile(&subtask) {
            return subtask;
        }

        // If gcode_file looks like an actual name, use it
        if !gcode.is_empty() && !self.looks_like_slicer_profile(&gcode) {
            return gcode;
        }

        // We only have slicer profile info - format it nicely
        let profile = if !subtask.is_empty() { &subtask } else { &gcode };

        if profile.is_empty() {
            return String::new();
        }

        // For cloud prints, prefix with "Cloud:" to make it clear
        if self.print_type == "cloud" {
            format!("Cloud: {}", profile)
        } else {
            profile.to_string()
        }
    }

    fn clean_name(&self, name: &str) -> String {
        name.trim()
            .trim_end_matches(".3mf")
            .trim_end_matches(".gcode")
            .trim_end_matches(".gcode.3mf")
            .to_string()
    }

    fn looks_like_slicer_profile(&self, name: &str) -> bool {
        let lower = name.to_lowercase();

        // Pattern: "0.2mm layer, 2 walls, 15% infill" style
        if lower.contains("mm layer") || lower.contains("% infill") || lower.contains(" walls") {
            return true;
        }

        // Pattern: contains multiple common profile terms
        let profile_terms = ["pla", "petg", "abs", "tpu", "draft", "quality", "strength"];
        let term_count = profile_terms.iter().filter(|t| lower.contains(*t)).count();
        term_count >= 2
    }
}

#[derive(Debug, Clone, Default)]
pub struct Temperatures {
    pub nozzle: f32,
    pub nozzle_target: f32,
    pub bed: f32,
    pub bed_target: f32,
    pub chamber: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Speeds {
    pub speed_level: u8,
    pub fan_speed: u8,
    pub aux_fan_speed: u8,
    pub chamber_fan_speed: u8,
}

#[derive(Debug, Clone, Default)]
pub struct AmsState {
    pub units: Vec<AmsUnit>,
    /// The currently active tray slot (0-3 within a unit)
    pub current_tray: Option<u8>,
    /// The currently active AMS unit index (0-3)
    pub current_unit: Option<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct AmsUnit {
    pub id: u8,
    pub humidity: u8,
    pub trays: Vec<AmsTray>,
    /// True if this is an AMS Lite unit (2 trays instead of 4)
    pub is_lite: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AmsTray {
    pub id: u8,
    pub material: String,
    pub color: String,
    pub remaining: u8,
}

#[derive(Debug, Clone, Default)]
pub struct LightState {
    pub chamber_light: bool,
    pub work_light: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct HmsError {
    pub code: u32,
    pub module: u8,
    pub severity: u8,
    pub message: String,
}

/// Raw MQTT message structure from Bambu printer
#[derive(Debug, Deserialize)]
pub struct MqttMessage {
    pub print: Option<PrintReport>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct PrintReport {
    // Print job info
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub profile_id: Option<String>,
    pub subtask_id: Option<String>,
    #[serde(rename = "mc_percent")]
    pub progress: Option<u8>,
    pub layer_num: Option<u32>,
    pub total_layer_num: Option<u32>,
    #[serde(rename = "mc_remaining_time")]
    pub remaining_time: Option<u32>,
    pub gcode_state: Option<String>,
    pub print_type: Option<String>,

    // Temperatures
    pub nozzle_temper: Option<f32>,
    pub nozzle_target_temper: Option<f32>,
    pub bed_temper: Option<f32>,
    pub bed_target_temper: Option<f32>,
    pub chamber_temper: Option<f32>,

    // Speeds & fans
    pub spd_lvl: Option<u8>,
    pub cooling_fan_speed: Option<String>,
    pub big_fan1_speed: Option<String>,
    pub big_fan2_speed: Option<String>,

    // Lights
    pub lights_report: Option<Vec<LightReport>>,

    // AMS
    pub ams: Option<AmsReport>,
    pub ams_status: Option<u32>,

    // Misc
    pub wifi_signal: Option<String>,

    // Printer info
    pub machine_name: Option<String>,
    pub hw_ver: Option<String>,
    pub sw_ver: Option<String>,

    // HMS errors
    pub hms: Option<Vec<HmsReport>>,
}

#[derive(Debug, Deserialize)]
pub struct LightReport {
    pub node: String,
    pub mode: String,
}

#[derive(Debug, Deserialize)]
pub struct AmsReport {
    pub ams: Option<Vec<AmsUnitReport>>,
    pub tray_now: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AmsUnitReport {
    pub id: String,
    pub humidity: String,
    pub tray: Option<Vec<AmsTrayReport>>,
}

#[derive(Debug, Deserialize)]
pub struct AmsTrayReport {
    pub id: String,
    pub tray_type: Option<String>,
    pub tray_color: Option<String>,
    pub remain: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct HmsReport {
    pub attr: u32,
    pub code: u32,
}

impl PrinterState {
    pub fn update_from_message(&mut self, msg: &MqttMessage) {
        if let Some(print) = &msg.print {
            self.update_from_print_report(print);
        }
    }

    fn update_from_print_report(&mut self, report: &PrintReport) {
        // Print status
        if let Some(v) = &report.gcode_file {
            self.print_status.gcode_file = v.clone();
        }
        if let Some(v) = &report.subtask_name {
            self.print_status.subtask_name = v.clone();
        }
        if let Some(v) = &report.project_id {
            self.print_status.project_id = v.clone();
        }
        if let Some(v) = &report.task_id {
            self.print_status.task_id = v.clone();
        }
        if let Some(v) = report.progress {
            self.print_status.progress = v;
        }
        if let Some(v) = report.layer_num {
            self.print_status.layer_num = v;
        }
        if let Some(v) = report.total_layer_num {
            self.print_status.total_layers = v;
        }
        if let Some(v) = report.remaining_time {
            self.print_status.remaining_time_mins = v;
        }
        if let Some(v) = &report.gcode_state {
            self.print_status.gcode_state = v.clone();
            self.gcode_state = v.clone();
        }
        if let Some(v) = &report.print_type {
            self.print_status.print_type = v.clone();
        }

        // Temperatures
        if let Some(v) = report.nozzle_temper {
            self.temperatures.nozzle = v;
        }
        if let Some(v) = report.nozzle_target_temper {
            self.temperatures.nozzle_target = v;
        }
        if let Some(v) = report.bed_temper {
            self.temperatures.bed = v;
        }
        if let Some(v) = report.bed_target_temper {
            self.temperatures.bed_target = v;
        }
        if let Some(v) = report.chamber_temper {
            self.temperatures.chamber = v;
        }

        // Speeds
        if let Some(v) = report.spd_lvl {
            self.speeds.speed_level = v;
        }
        if let Some(v) = &report.cooling_fan_speed {
            self.speeds.fan_speed = parse_fan_speed(v);
        }
        if let Some(v) = &report.big_fan1_speed {
            self.speeds.aux_fan_speed = parse_fan_speed(v);
        }
        if let Some(v) = &report.big_fan2_speed {
            self.speeds.chamber_fan_speed = parse_fan_speed(v);
        }

        // Lights
        if let Some(lights) = &report.lights_report {
            for light in lights {
                match light.node.as_str() {
                    "chamber_light" => self.lights.chamber_light = light.mode == "on",
                    "work_light" => self.lights.work_light = light.mode == "on",
                    _ => {}
                }
            }
        }

        // WiFi signal - may come as "-65dBm" or just "-65" or as percentage
        if let Some(v) = &report.wifi_signal {
            // Strip non-numeric chars except minus sign
            let cleaned: String = v.chars().filter(|c| c.is_ascii_digit() || *c == '-').collect();
            self.wifi_signal = cleaned.parse().unwrap_or(0);
        }

        // AMS
        if let Some(ams_report) = &report.ams {
            self.update_ams(ams_report);
        }

        // HMS errors
        if let Some(hms_list) = &report.hms {
            self.hms_errors = hms_list
                .iter()
                .map(|h| HmsError {
                    code: h.code,
                    module: ((h.attr >> 24) & 0xFF) as u8,
                    severity: ((h.attr >> 16) & 0xFF) as u8,
                    message: format_hms_code(h.code),
                })
                .collect();
        }

        // Printer info
        if let Some(v) = &report.machine_name {
            self.printer_name = v.clone();
        }
        if let Some(v) = &report.hw_ver {
            self.printer_model = model_from_hw_ver(v);
        }
    }

    /// Set model from serial number prefix if not already set from MQTT
    pub fn set_model_from_serial(&mut self, serial: &str) {
        if self.printer_model.is_empty() {
            self.printer_model = model_from_serial(serial);
        }
    }

    /// Returns true if this printer model has a chamber temperature sensor.
    /// X1 series, P2S, and H2 series have sensors.
    /// P1P, P1S, A1, and A1 Mini do not.
    pub fn has_chamber_temp_sensor(&self) -> bool {
        let model = self.printer_model.to_uppercase();
        model.contains("X1C")
            || model.contains("X1E")
            || model.contains("P2S")
            || model.contains("H2C")
            || model.contains("H2S")
            || model.contains("H2D")
    }

    fn update_ams(&mut self, report: &AmsReport) {
        let mut ams_state = self.ams.take().unwrap_or_default();

        // Parse tray_now to determine both active unit and tray slot
        // Format: tray_now is a combined value where:
        // - For AMS: value = (unit_id * 4) + tray_id (e.g., "5" = unit 1, tray 1)
        // - Special values: "254" = external spool, "255" = no tray selected
        if let Some(tray) = &report.tray_now {
            if let Ok(tray_val) = tray.parse::<u8>() {
                if tray_val < 254 {
                    // Calculate unit and slot from combined tray value
                    ams_state.current_unit = Some(tray_val / 4);
                    ams_state.current_tray = Some(tray_val % 4);
                } else {
                    // External spool or no selection
                    ams_state.current_unit = None;
                    ams_state.current_tray = None;
                }
            }
        }

        if let Some(units) = &report.ams {
            ams_state.units = units
                .iter()
                .map(|u| {
                    let trays: Vec<AmsTray> = u
                        .tray
                        .as_ref()
                        .map(|trays| {
                            trays
                                .iter()
                                .map(|t| AmsTray {
                                    id: t.id.parse().unwrap_or(0),
                                    material: t.tray_type.clone().unwrap_or_default(),
                                    color: t.tray_color.clone().unwrap_or_default(),
                                    remaining: t.remain.unwrap_or(0).max(0) as u8,
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    // Detect AMS Lite: has only 2 tray slots instead of 4
                    // AMS Lite units report fewer trays or have humidity value of 0
                    let is_lite = trays.len() <= 2 && !trays.is_empty();

                    AmsUnit {
                        id: u.id.parse().unwrap_or(0),
                        humidity: u.humidity.parse().unwrap_or(0),
                        trays,
                        is_lite,
                    }
                })
                .collect();
        }

        self.ams = Some(ams_state);
    }
}

fn parse_fan_speed(s: &str) -> u8 {
    // Fan speed comes as a string like "15" representing 0-15 scale
    // Convert to percentage
    let val: u8 = s.parse().unwrap_or(0);
    ((val as f32 / 15.0) * 100.0) as u8
}

fn format_hms_code(code: u32) -> String {
    // HMS error code lookup - common codes from Bambu documentation
    match code {
        // AMS errors (0x0700xxxx)
        0x0700_0001 => "AMS: Filament runout".to_string(),
        0x0700_0002 => "AMS: Filament broken".to_string(),
        0x0700_0003 => "AMS: Filament tangled".to_string(),
        0x0700_0004 => "AMS: Filament unloading failed".to_string(),
        0x0700_0005 => "AMS: Filament loading failed".to_string(),
        0x0700_0006 => "AMS: Slot empty".to_string(),
        0x0700_0100 => "AMS: Assist motor overload".to_string(),
        0x0700_0200 => "AMS: Cutter error".to_string(),
        0x0700_0300 => "AMS: Filament may be tangled".to_string(),
        0x0700_0400 => "AMS: RFID read error".to_string(),
        0x0700_0500 => "AMS: AMS communication error".to_string(),
        0x0700_1000 => "AMS: Humidity sensor error".to_string(),

        // Nozzle/hotend errors (0x0300xxxx)
        0x0300_0001 => "Nozzle: Temperature too high".to_string(),
        0x0300_0002 => "Nozzle: Temperature too low".to_string(),
        0x0300_0003 => "Nozzle: Temperature abnormal".to_string(),
        0x0300_0100 => "Nozzle: Heater error".to_string(),
        0x0300_0200 => "Nozzle: Thermistor error".to_string(),
        0x0300_0300 => "Nozzle: Clogged".to_string(),

        // Bed errors (0x0400xxxx)
        0x0400_0001 => "Bed: Temperature too high".to_string(),
        0x0400_0002 => "Bed: Temperature too low".to_string(),
        0x0400_0100 => "Bed: Heater error".to_string(),
        0x0400_0200 => "Bed: Thermistor error".to_string(),

        // Motion errors (0x0500xxxx)
        0x0500_0001 => "Motion: X-axis homing failed".to_string(),
        0x0500_0002 => "Motion: Y-axis homing failed".to_string(),
        0x0500_0003 => "Motion: Z-axis homing failed".to_string(),
        0x0500_0100 => "Motion: X-axis motor error".to_string(),
        0x0500_0200 => "Motion: Y-axis motor error".to_string(),
        0x0500_0300 => "Motion: Z-axis motor error".to_string(),
        0x0500_0400 => "Motion: Extruder motor error".to_string(),

        // Print errors (0x0C00xxxx)
        0x0C00_0001 => "Print: First layer inspection failed".to_string(),
        0x0C00_0002 => "Print: Spaghetti detected".to_string(),
        0x0C00_0003 => "Print: Foreign object on bed".to_string(),
        0x0C00_0100 => "Print: Build plate not detected".to_string(),
        0x0C00_0200 => "Print: Auto-leveling failed".to_string(),
        0x0C00_0300 => "Print: Nozzle height abnormal".to_string(),

        // System errors (0x0800xxxx)
        0x0800_0001 => "System: SD card error".to_string(),
        0x0800_0002 => "System: Storage full".to_string(),
        0x0800_0100 => "System: Camera error".to_string(),
        0x0800_0200 => "System: WiFi disconnected".to_string(),
        0x0800_0300 => "System: Chamber door open".to_string(),
        0x0800_0400 => "System: Front cover removed".to_string(),

        // Fallback for unknown codes
        _ => format!("Error: 0x{:08X}", code),
    }
}

fn model_from_serial(serial: &str) -> String {
    // Bambu serial number prefixes indicate model
    // Format: XXYYYZZ... where XX indicates model
    if serial.len() < 3 {
        return "Bambu Printer".to_string();
    }

    // Prefixes from: https://wiki.bambulab.com/en/general/find-sn
    // Note: 01P/01S are counterintuitively swapped (01P=P1S, 01S=P1P)
    match &serial[..3] {
        // P1 Series
        "01P" => "Bambu Lab P1S".to_string(),
        "01S" => "Bambu Lab P1P".to_string(),
        "22E" => "Bambu Lab P2S".to_string(),
        // X1 Series
        "00M" => "Bambu Lab X1C".to_string(),
        "03W" => "Bambu Lab X1E".to_string(),
        // A1 Series
        "030" => "Bambu Lab A1 Mini".to_string(),
        "039" => "Bambu Lab A1".to_string(),
        // H2 Series
        "31B" => "Bambu Lab H2C".to_string(),
        "093" => "Bambu Lab H2S".to_string(),
        "094" => "Bambu Lab H2D".to_string(),
        "239" => "Bambu Lab H2D Pro".to_string(),
        _ => "Bambu Printer".to_string(),
    }
}

fn model_from_hw_ver(hw_ver: &str) -> String {
    // hw_ver might contain model info like "AP05" for A1, etc.
    // Check more specific patterns first to avoid substring false matches
    let hw = hw_ver.to_uppercase();

    // P Series
    if hw.contains("P1P") {
        "Bambu Lab P1P".to_string()
    } else if hw.contains("P1S") {
        "Bambu Lab P1S".to_string()
    } else if hw.contains("P2S") {
        "Bambu Lab P2S".to_string()
    // X Series
    } else if hw.contains("X1C") {
        "Bambu Lab X1C".to_string()
    } else if hw.contains("X1E") {
        "Bambu Lab X1E".to_string()
    // A Series (check A1M before A1)
    } else if hw.contains("A1M") || hw.contains("A1 MINI") {
        "Bambu Lab A1 Mini".to_string()
    } else if hw.contains("A1") {
        "Bambu Lab A1".to_string()
    // H2 Series (check longer patterns first)
    } else if hw.contains("H2D PRO") {
        "Bambu Lab H2D Pro".to_string()
    } else if hw.contains("H2D") {
        "Bambu Lab H2D".to_string()
    } else if hw.contains("H2C") {
        "Bambu Lab H2C".to_string()
    } else if hw.contains("H2S") {
        "Bambu Lab H2S".to_string()
    } else {
        // Return hw_ver itself if we can't map it
        format!("Bambu {}", hw_ver)
    }
}
