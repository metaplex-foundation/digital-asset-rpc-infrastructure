use crate::{
    dao::sea_orm_active_enums::EndSettingType,
    rpc::{ConfigLineSettings, EndSettings},
};

// TODO make get all settings methods and get all guards methods using these

// All of these helpers are essentially transforming the separate Option<> fields
// into their optionable struct types to be returned from rpc

pub fn get_end_settings(
    end_setting_number: Option<u64>,
    end_setting_type: Option<EndSettingType>,
) -> Option<EndSettings> {
    if let (Some(end_setting_number), Some(end_setting_type)) =
        (end_setting_number, end_setting_type)
    {
        Some(EndSettings {
            end_setting_type,
            number: end_setting_number,
        })
    } else {
        None
    }
}

pub fn get_config_line_settings(
    is_sequential: Option<bool>,
    name_length: Option<u32>,
    prefix_name: Option<String>,
    prefix_uri: Option<String>,
    uri_length: Option<u32>,
) -> Option<ConfigLineSettings> {
    if let (
        Some(is_sequential),
        Some(name_length),
        Some(prefix_name),
        Some(prefix_uri),
        Some(uri_length),
    ) = (
        is_sequential,
        name_length,
        prefix_name,
        prefix_uri,
        uri_length,
    ) {
        Some(ConfigLineSettings {
            prefix_name,
            name_length,
            prefix_uri,
            uri_length,
            is_sequential,
        })
    } else {
        None
    }
}
