use multitor_ipc::{ProductCapabilities, ProductEdition, MAX_SUPPORTED_VIRTUAL_DISPLAYS};

pub const ACTIVE_CAPABILITIES: ProductCapabilities = ProductCapabilities {
    edition: ProductEdition::Lite,
    // Lite has no artificial product limit. The remaining cap is the maximum
    // implemented by the signed MttVDD driver protocol.
    max_virtual_displays: MAX_SUPPORTED_VIRTUAL_DISPLAYS,
    multi_display_layouts: true,
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
    fn lite_exposes_every_display_supported_by_the_driver() {
        let capabilities = ACTIVE_CAPABILITIES;
        assert_eq!(
            capabilities.max_virtual_displays,
            MAX_SUPPORTED_VIRTUAL_DISPLAYS
        );
        assert!(capabilities.multi_display_layouts);
        assert!(!capabilities.cloud_features);
    }

    #[test]
    fn every_driver_supported_display_count_is_accepted() {
        for count in 0..=MAX_SUPPORTED_VIRTUAL_DISPLAYS {
            assert!(validate_virtual_display_count(count).is_ok());
        }
        assert!(validate_virtual_display_count(MAX_SUPPORTED_VIRTUAL_DISPLAYS + 1).is_err());
    }
}
