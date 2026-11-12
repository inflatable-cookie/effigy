//! The image platform one verified container runs, read from the backend's own
//! image inspect. Nothing is derived from the host or from the guest's report.

use std::path::Path;
use std::time::Instant;

use effigy_containers::EffectiveContainerPolicy;

/// What the backend reports about the exact image one verified container runs.
///
/// These are the image's own facts, read from the image the container names:
/// the host never derives them from the guest's `uname`, from its own
/// architecture or from the artifact it wants to publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct ObservedImagePlatform {
    pub(in crate::runner) os: String,
    pub(in crate::runner) architecture: String,
    pub(in crate::runner) variant: String,
}

/// Observe the image platform behind one verified container id.
///
/// Two bounded runtime reads: the container's own inspect names its image, then
/// the image's inspect reports the OS, architecture and variant. A capture that
/// fails, a missing or non-string field, or an expired deadline is `Unknown`; no
/// default, host architecture or guest report stands in for it.
pub(in crate::runner) fn observed_image_platform(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_id: &str,
    deadline: Instant,
) -> effigy_kernel::observe::Observed<ObservedImagePlatform> {
    use effigy_kernel::observe::{Evidence, Observed, UnknownReason};

    let unknown = |reason: String| Observed::Unknown(UnknownReason::Inconsistent(reason));
    let container = match effigy_containers::exec::inspect_containers_for_policy(
        repo_root,
        policy,
        std::slice::from_ref(&container_id.to_owned()),
        Some(deadline),
    ) {
        Ok(output) => output.stdout,
        Err(error) => return unknown(format!("container inspect is unavailable: {error}")),
    };
    let image = match image_reference(&container) {
        Ok(image) => image,
        Err(reason) => return unknown(reason),
    };
    let inspected = match effigy_containers::exec::inspect_image_for_policy(
        repo_root,
        policy,
        &image,
        Some(deadline),
    ) {
        Ok(output) => output.stdout,
        Err(error) => return unknown(format!("image inspect is unavailable: {error}")),
    };
    let platform = match parse_image_platform(&inspected) {
        Ok(platform) => platform,
        Err(reason) => return unknown(reason),
    };
    Observed::Verified(
        platform,
        Evidence::new("backend image inspect".into(), None).expect("fixed evidence"),
    )
}

/// The image one container inspect record names.
pub(super) fn image_reference(container_inspect: &[u8]) -> Result<String, String> {
    serde_json::from_slice::<serde_json::Value>(container_inspect)
        .ok()
        .and_then(|records| {
            records
                .as_array()?
                .first()?
                .get("Image")?
                .as_str()
                .map(str::to_owned)
        })
        .map(|image| image.trim().to_owned())
        .filter(|image| !image.is_empty())
        .ok_or_else(|| "the container inspect names no image".to_owned())
}

/// The platform one image inspect record reports. A missing or non-string OS or
/// architecture is unknown; the variant may legitimately be absent.
pub(super) fn parse_image_platform(image_inspect: &[u8]) -> Result<ObservedImagePlatform, String> {
    let record = serde_json::from_slice::<serde_json::Value>(image_inspect)
        .ok()
        .and_then(|records| records.as_array()?.first().cloned())
        .ok_or_else(|| "the image inspect returned no record".to_owned())?;
    let field = |name: &str| {
        record
            .get(name)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let (Some(os), Some(architecture)) = (field("Os"), field("Architecture")) else {
        return Err("the image inspect names no OS or architecture".to_owned());
    };
    Ok(ObservedImagePlatform {
        os,
        architecture,
        variant: field("Variant").unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::{image_reference, parse_image_platform, ObservedImagePlatform};

    #[test]
    fn the_observed_image_platform_reads_the_backend_report_and_refuses_the_rest() {
        let container = br#"[{"Id": "9d1c0b6a7f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4", "Image": "sha256:abc"}]"#;
        let image = br#"[{"Os": "linux", "Architecture": "arm64", "Variant": "v8"}]"#;
        assert_eq!(
            image_reference(container).expect("the container names its image"),
            "sha256:abc"
        );
        assert_eq!(
            parse_image_platform(image).expect("a complete image report"),
            ObservedImagePlatform {
                os: "linux".into(),
                architecture: "arm64".into(),
                variant: "v8".into(),
            }
        );
        // An image that reports no variant is that image's answer, not an omission
        // this host may fill in.
        assert_eq!(
            parse_image_platform(br#"[{"Os": "linux", "Architecture": "amd64"}]"#)
                .expect("variant is optional")
                .variant,
            ""
        );
        // A container record that names no image, and an image record with no OS or
        // architecture, are unknown rather than defaulted.
        assert!(image_reference(br#"[{"Id": "9d1c"}]"#).is_err());
        assert!(image_reference(b"not json").is_err());
        assert!(parse_image_platform(br#"[{"Os": "linux"}]"#).is_err());
        assert!(parse_image_platform(br#"[{"Os": "linux", "Architecture": 7}]"#).is_err());
        assert!(parse_image_platform(b"[]").is_err());
    }
}
