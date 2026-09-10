use multitor_ipc::{ProductCapabilities, ProductEdition};

pub const ACTIVE_CAPABILITIES: ProductCapabilities = ProductCapabilities {
    edition: ProductEdition::Lite,
    max_virtual_displays: 1,
    multi_display_layouts: false,
    cloud_features: false,
};

pub fn validate_virtual_display_count(count: u32) -> Result<(), String> {
    if count <= ACTIVE_CAPABILITIES.max_virtual_displays {
        Ok(())
    } else {
        Err(format!(
            "{} edition supports at most {} active virtual display(s)",
            ACTIVE_CAPABILITIES.edition.as_str(),
            ACTIVE_CAPABILITIES.max_virtual_displays
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lite_is_authoritatively_limited_to_one_virtual_display() {
        let capabilities = ACTIVE_CAPABILITIES;
        assert_eq!(capabilities.max_virtual_displays, 1);
        assert!(!capabilities.multi_display_layouts);
        assert!(!capabilities.cloud_features);
    }
}
