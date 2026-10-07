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

#[path = "../../../src-tauri/src/preset_converter.rs"]
pub mod preset_converter;

#[path = "../../../src-tauri/src/sony_white_balance.rs"]
pub mod sony_white_balance;

#[path = "../../../src-tauri/src/camera_calibration.rs"]
pub mod camera_calibration;

#[path = "../../../src-tauri/src/imported_curve.rs"]
pub mod imported_curve;
