mod errors;
mod metadata;
mod oci;
mod refs;
mod reports;
mod staging;
mod util;

pub use errors::{ArtifactRefError, ArtifactStagingError, OciArtifactError};
pub use metadata::{ArtifactMetadata, ARTIFACT_METADATA_SCHEMA};
pub use oci::{
    parse_oras_descriptor, parse_oras_pull_files, sanitize_process_output, OciArtifactAdapter,
    OciArtifactDescriptor, OciArtifactInspectRequest, OciArtifactPullReport,
    OciArtifactPullRequest, OciArtifactPushReport, OciArtifactPushRequest, OrasCliArtifactAdapter,
};
pub use refs::{
    ArtifactKind, ArtifactSourceRef, ArtifactSourceType, LocalArtifactRef, OciArtifactRef,
};
pub use reports::{ArtifactOperation, ArtifactOperationReport, ArtifactOperationResult};
pub use staging::{
    default_local_artifact_root, stage_local_artifact, stage_oci_artifact,
    LocalArtifactStagingRequest, OciArtifactStagingRequest, StagedArtifactReport,
};

#[cfg(test)]
mod tests {
    use super::{
        default_local_artifact_root, stage_local_artifact, stage_oci_artifact, ArtifactKind,
        ArtifactMetadata, ArtifactRefError, ArtifactSourceRef, LocalArtifactStagingRequest,
        OciArtifactDescriptor, OciArtifactStagingRequest, ARTIFACT_METADATA_SCHEMA,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "effigy-artifacts-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn parses_local_sql_ref() {
        let parsed = ArtifactSourceRef::parse("./backups/site.sql").expect("parse ref");

        let ArtifactSourceRef::Local(local) = parsed else {
            panic!("expected local ref");
        };
        assert_eq!(local.path(), PathBuf::from("./backups/site.sql").as_path());
        assert_eq!(local.inferred_kind(), Some(ArtifactKind::SqlDump));
    }

    #[test]
    fn parses_local_compressed_sql_ref() {
        let parsed = ArtifactSourceRef::parse("./backups/site.sql.gz").expect("parse ref");

        let ArtifactSourceRef::Local(local) = parsed else {
            panic!("expected local ref");
        };
        assert_eq!(local.inferred_kind(), Some(ArtifactKind::SqlDump));
    }

    #[test]
    fn parses_local_dump_ref() {
        let parsed = ArtifactSourceRef::parse("/tmp/site.dump").expect("parse ref");

        let ArtifactSourceRef::Local(local) = parsed else {
            panic!("expected local ref");
        };
        assert_eq!(local.inferred_kind(), Some(ArtifactKind::SqlDump));
    }

    #[test]
    fn parses_explicit_oci_ref() {
        let parsed = ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/legacy-cbs@sha256:abc123")
            .expect("parse ref");

        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci ref");
        };
        assert_eq!(
            oci.reference(),
            "ghcr.io/acowtancy/legacy-cbs@sha256:abc123"
        );
        assert!(oci.is_digest_pinned());
    }

    #[test]
    fn rejects_unprefixed_registry_like_ref() {
        let error = ArtifactSourceRef::parse("ghcr.io/acowtancy/legacy-cbs:latest")
            .expect_err("reject ambiguous ref");

        assert_eq!(
            error,
            ArtifactRefError::AmbiguousOciReference {
                value: "ghcr.io/acowtancy/legacy-cbs:latest".to_owned()
            }
        );
    }

    #[test]
    fn rejects_empty_oci_ref() {
        let error = ArtifactSourceRef::parse("oci://").expect_err("reject missing ref");

        assert_eq!(error, ArtifactRefError::MissingOciReference);
    }

    #[test]
    fn builds_metadata_with_schema_and_source() {
        let source = ArtifactSourceRef::parse("./backups/site.sql.gz").expect("parse ref");
        let metadata = ArtifactMetadata::new(
            ArtifactKind::SqlDump,
            &source,
            PathBuf::from(".effigy/local/artifacts/site"),
            vec![PathBuf::from(".effigy/local/artifacts/site/site.sql.gz")],
        )
        .with_digest("sha256:abc")
        .with_environment_label("uat");

        assert_eq!(metadata.schema, ARTIFACT_METADATA_SCHEMA);
        assert_eq!(metadata.kind, ArtifactKind::SqlDump);
        assert_eq!(metadata.source, "./backups/site.sql.gz");
        assert_eq!(metadata.digest.as_deref(), Some("sha256:abc"));
        assert_eq!(metadata.environment_label.as_deref(), Some("uat"));
    }

    #[test]
    fn stages_local_sql_artifact_with_metadata() {
        let repo = temp_dir();
        let backups = repo.join("backups");
        fs::create_dir_all(&backups).expect("create backups dir");
        fs::write(backups.join("site.sql.gz"), b"compressed sql").expect("write source");

        let source = ArtifactSourceRef::parse("./backups/site.sql.gz").expect("parse ref");
        let ArtifactSourceRef::Local(local) = source else {
            panic!("expected local source");
        };
        let request = LocalArtifactStagingRequest::new(
            local,
            repo.clone(),
            default_local_artifact_root(&repo),
        )
        .with_environment_label("uat");

        let report = stage_local_artifact(&request).expect("stage artifact");

        assert!(report
            .staged_root()
            .starts_with(repo.join(".effigy/local/artifacts")));
        assert_eq!(report.metadata.schema, ARTIFACT_METADATA_SCHEMA);
        assert_eq!(report.metadata.kind, ArtifactKind::SqlDump);
        assert_eq!(report.metadata.environment_label.as_deref(), Some("uat"));
        assert_eq!(report.metadata.primary_files.len(), 1);
        assert_eq!(
            fs::read(&report.metadata.primary_files[0]).expect("read staged payload"),
            b"compressed sql"
        );
        assert!(report.metadata_path.is_file());

        let metadata_json = fs::read_to_string(&report.metadata_path).expect("read metadata");
        assert!(metadata_json.contains("\"schema\": \"effigy.artifact.v1\""));
        assert!(metadata_json.contains("\"source\": \"./backups/site.sql.gz\""));
    }

    #[test]
    fn uses_deterministic_staging_root_for_same_source() {
        let repo = temp_dir();
        fs::write(repo.join("seed.sql"), b"select 1;").expect("write source");

        let source = ArtifactSourceRef::parse("seed.sql").expect("parse ref");
        let ArtifactSourceRef::Local(local) = source else {
            panic!("expected local source");
        };
        let request = LocalArtifactStagingRequest::new(
            local,
            repo.clone(),
            default_local_artifact_root(&repo),
        );

        let first = stage_local_artifact(&request).expect("first stage");
        let second = stage_local_artifact(&request).expect("second stage");

        assert_eq!(first.metadata.staged_root, second.metadata.staged_root);
        assert_eq!(first.metadata_path, second.metadata_path);
        assert_eq!(first.metadata.primary_files, second.metadata.primary_files);
    }

    #[test]
    fn rejects_missing_local_source_file() {
        let repo = temp_dir();
        let source = ArtifactSourceRef::parse("missing.sql").expect("parse ref");
        let ArtifactSourceRef::Local(local) = source else {
            panic!("expected local source");
        };
        let request = LocalArtifactStagingRequest::new(
            local,
            repo.clone(),
            default_local_artifact_root(&repo),
        );

        let error = stage_local_artifact(&request).expect_err("reject missing source");

        assert!(error.to_string().contains("artifact source is not a file"));
    }

    #[test]
    fn redacts_oci_userinfo_from_reportable_ref() {
        let parsed =
            ArtifactSourceRef::parse("oci://token:secret@ghcr.io/acowtancy/private:latest")
                .expect("parse ref");

        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci source");
        };

        assert_eq!(oci.redacted(), "***@ghcr.io/acowtancy/private:latest");
        assert_eq!(
            ArtifactSourceRef::Oci(oci.clone()).display_ref(),
            "oci://***@ghcr.io/acowtancy/private:latest"
        );
        let descriptor = OciArtifactDescriptor::new(&oci);
        assert_eq!(descriptor.reference, "***@ghcr.io/acowtancy/private:latest");
    }

    #[test]
    fn descriptor_captures_digest_from_ref() {
        let parsed = ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/private@sha256:abc123")
            .expect("parse ref");

        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci source");
        };
        let descriptor = OciArtifactDescriptor::new(&oci);

        assert_eq!(descriptor.digest.as_deref(), Some("sha256:abc123"));
        assert_eq!(
            descriptor.redacted_reference,
            "ghcr.io/acowtancy/private@sha256:abc123"
        );
    }

    #[test]
    fn stages_pulled_oci_artifact_with_same_metadata_model() {
        let repo = temp_dir();
        let pulled_root = repo.join("pulled");
        fs::create_dir_all(&pulled_root).expect("create pulled root");
        fs::write(pulled_root.join("legacy.sql"), b"create table legacy;")
            .expect("write pulled payload");

        let parsed = ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/legacy@sha256:abc123")
            .expect("parse ref");
        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci source");
        };
        let request = OciArtifactStagingRequest::new(
            oci,
            pulled_root,
            default_local_artifact_root(&repo),
            vec![PathBuf::from("legacy.sql")],
            ArtifactKind::LegacySourceSnapshot,
        )
        .with_digest("sha256:abc123")
        .with_environment_label("uat");

        let report = stage_oci_artifact(&request).expect("stage oci artifact");

        assert_eq!(report.metadata.schema, ARTIFACT_METADATA_SCHEMA);
        assert_eq!(report.metadata.kind, ArtifactKind::LegacySourceSnapshot);
        assert_eq!(
            report.metadata.source,
            "oci://ghcr.io/acowtancy/legacy@sha256:abc123"
        );
        assert_eq!(report.metadata.digest.as_deref(), Some("sha256:abc123"));
        assert_eq!(report.metadata.environment_label.as_deref(), Some("uat"));
        assert_eq!(report.metadata.primary_files.len(), 1);
        assert_eq!(
            fs::read(&report.metadata.primary_files[0]).expect("read staged payload"),
            b"create table legacy;"
        );
        assert!(report.metadata_path.is_file());
    }

    #[test]
    fn rejects_oci_stage_without_primary_files() {
        let repo = temp_dir();
        let parsed =
            ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/legacy:latest").expect("parse ref");
        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci source");
        };
        let request = OciArtifactStagingRequest::new(
            oci,
            repo.join("pulled"),
            default_local_artifact_root(&repo),
            Vec::new(),
            ArtifactKind::AppSpecific,
        );

        let error = stage_oci_artifact(&request).expect_err("reject missing primary files");

        assert_eq!(error.to_string(), "artifact has no primary files to stage");
    }
}
