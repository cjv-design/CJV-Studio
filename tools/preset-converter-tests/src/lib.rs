// Compile the production converter and its own unit tests without linking the
// GPU/editor runtime. The Windows installer build separately validates the full
// application's concrete Preset type and white-balance module.
pub mod file_management {
    pub struct Preset {
        pub id: String,
        pub name: String,
        pub adjustments: serde_json::Value,
        pub include_masks: Option<bool>,
        pub include_crop_transform: Option<bool>,
        pub preset_type: Option<String>,
    }
}

pub mod white_balance {
    pub const MIRED_PER_RELATIVE_UNIT: f64 = 1.5;
    pub const TINT_PER_RELATIVE_UNIT: f64 = 1.5;
}

#[path = "../../../src-tauri/src/preset_converter.rs"]
pub mod preset_converter;
