use std::collections::{BTreeMap, BTreeSet};

use crate::artifacts::{
    FILE_OPERATION_SEMANTICS_VERSION, FIXTURE_REPOSITORY_ID, FIXTURE_TREE_HASH,
    POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
};

pub const FIXTURE_BASE_CHECKPOINT_ID: &str = "checkpoint_base_0001";
pub const FIXTURE_BASE_RESOLVED_VIEW_ID: &str = "view_base_0001";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingleRepoTree {
    pub repository_id: String,
    pub tree_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverInputFrontier {
    pub repository_id: String,
    pub base_checkpoint_ids: Vec<String>,
    pub topic_frontier: Vec<TopicRevisionSelection>,
    pub operation_semantics_version: String,
    pub path_policy_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TopicRevisionSelection {
    pub topic_id: String,
    pub revision_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyClosure {
    pub revision_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicResolverOrder {
    pub operation_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedViewResult {
    pub resolved_view_id: String,
    pub repository_id: String,
    pub base_checkpoint_ids: Vec<String>,
    pub topic_frontier: BTreeMap<String, String>,
    pub dependency_closure: DependencyClosure,
    pub operation_semantics_version: String,
    pub path_policy_id: String,
    pub resolver_order: DeterministicResolverOrder,
    pub tree_identity: Option<SingleRepoTree>,
    pub records: Vec<ResolverConflictOrStalenessRecord>,
    pub tree_entries: BTreeMap<String, TreeEntryState>,
}

impl ResolvedViewResult {
    pub fn conflict_free(&self) -> bool {
        self.records.is_empty() && self.tree_identity.is_some()
    }

    pub fn conflicts(&self) -> impl Iterator<Item = &ResolverConflictOrStalenessRecord> {
        self.records
            .iter()
            .filter(|record| record.kind.is_conflict())
    }

    pub fn staleness(&self) -> impl Iterator<Item = &ResolverConflictOrStalenessRecord> {
        self.records
            .iter()
            .filter(|record| record.kind.is_staleness())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverConflictOrStalenessRecord {
    pub id: String,
    pub kind: ResolverRecordKind,
    pub resolved_view_id: String,
    pub artifact_ids: Vec<String>,
    pub path_refs: Vec<PathRef>,
    pub operation_ids: Vec<String>,
    pub authored_context_ids: Vec<String>,
    pub policy_reason: String,
    pub candidate_refs: BTreeMap<String, Vec<String>>,
    pub resolution_operation_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolverRecordKind {
    SameArtifactConflict,
    MissingDependency,
    StaleDependency,
    FrontierInconsistent,
}

impl ResolverRecordKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SameArtifactConflict => "same_artifact_conflict",
            Self::MissingDependency => "missing_dependency",
            Self::StaleDependency => "stale_dependency",
            Self::FrontierInconsistent => "frontier_inconsistent",
        }
    }

    pub fn is_conflict(self) -> bool {
        matches!(
            self,
            Self::SameArtifactConflict | Self::FrontierInconsistent
        )
    }

    pub fn is_staleness(self) -> bool {
        matches!(self, Self::MissingDependency | Self::StaleDependency)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRef {
    pub operation_transaction_id: String,
    pub topic_id: String,
    pub topic_revision_id: String,
    pub artifact_id: String,
    pub path: String,
    pub mutation: ResolverMutationKind,
    pub base_content_hash: Option<String>,
    pub result_content_hash: String,
    pub authored_context_id: String,
}

impl OperationRef {
    fn canonical_order_key(&self, repository_id: &str) -> OperationOrderKey {
        OperationOrderKey {
            repository_id: repository_id.to_string(),
            path: self.path.clone(),
            artifact_id: self.artifact_id.clone(),
            topic_id: self.topic_id.clone(),
            topic_revision_id: self.topic_revision_id.clone(),
            operation_transaction_id: self.operation_transaction_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolverMutationKind {
    Patch,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicRevisionRef {
    pub topic_id: String,
    pub revision_id: String,
    pub operation: OperationRef,
    pub dependency_revision_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntryState {
    pub artifact_id: String,
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRef {
    pub path: String,
    pub path_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct OperationOrderKey {
    repository_id: String,
    path: String,
    artifact_id: String,
    topic_id: String,
    topic_revision_id: String,
    operation_transaction_id: String,
}

pub fn resolve_fixture_view(
    input: ResolverInputFrontier,
    base_entries: impl IntoIterator<Item = TreeEntryState>,
    revision_refs: impl IntoIterator<Item = TopicRevisionRef>,
) -> ResolvedViewResult {
    let normalized_frontier = normalize_frontier(&input.topic_frontier);
    let resolved_view_id = resolved_view_id(&input, &normalized_frontier);
    let base_entries = base_entries
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let revision_refs = revision_refs
        .into_iter()
        .map(|revision| (revision.revision_id.clone(), revision))
        .collect::<BTreeMap<_, _>>();

    let (dependency_closure, mut records) =
        close_dependencies(&resolved_view_id, &normalized_frontier, &revision_refs);

    let selected_revisions = normalized_frontier
        .values()
        .filter_map(|revision_id| revision_refs.get(revision_id))
        .collect::<Vec<_>>();
    records.extend(same_artifact_conflicts(
        &resolved_view_id,
        &selected_revisions,
    ));

    if !records.is_empty() {
        return ResolvedViewResult {
            resolved_view_id,
            repository_id: input.repository_id,
            base_checkpoint_ids: input.base_checkpoint_ids,
            topic_frontier: normalized_frontier,
            dependency_closure,
            operation_semantics_version: input.operation_semantics_version,
            path_policy_id: input.path_policy_id,
            resolver_order: DeterministicResolverOrder {
                operation_ids: Vec::new(),
            },
            tree_identity: None,
            records,
            tree_entries: base_entries,
        };
    }

    let ordered_operations = deterministic_operations(&input.repository_id, selected_revisions);
    let mut tree_entries = base_entries;
    for operation in &ordered_operations {
        tree_entries.insert(
            operation.path.clone(),
            TreeEntryState {
                artifact_id: operation.artifact_id.clone(),
                path: operation.path.clone(),
                content_hash: operation.result_content_hash.clone(),
            },
        );
    }

    let tree_identity = SingleRepoTree {
        repository_id: input.repository_id.clone(),
        tree_hash: tree_hash(&input.repository_id, &tree_entries),
    };

    ResolvedViewResult {
        resolved_view_id,
        repository_id: input.repository_id,
        base_checkpoint_ids: input.base_checkpoint_ids,
        topic_frontier: normalized_frontier,
        dependency_closure,
        operation_semantics_version: input.operation_semantics_version,
        path_policy_id: input.path_policy_id,
        resolver_order: DeterministicResolverOrder {
            operation_ids: ordered_operations
                .iter()
                .map(|operation| operation.operation_transaction_id.clone())
                .collect(),
        },
        tree_identity: Some(tree_identity),
        records,
        tree_entries,
    }
}

pub fn fixture_resolver_input(frontier: Vec<TopicRevisionSelection>) -> ResolverInputFrontier {
    ResolverInputFrontier {
        repository_id: FIXTURE_REPOSITORY_ID.to_string(),
        base_checkpoint_ids: vec![FIXTURE_BASE_CHECKPOINT_ID.to_string()],
        topic_frontier: frontier,
        operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
        path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
    }
}

pub fn fixture_base_entries() -> Vec<TreeEntryState> {
    vec![
        TreeEntryState {
            artifact_id: "artifact_src_auth_ts".to_string(),
            path: "src/auth.ts".to_string(),
            content_hash: "sha256:auth_base".to_string(),
        },
        TreeEntryState {
            artifact_id: "artifact_src_routes_ts".to_string(),
            path: "src/routes.ts".to_string(),
            content_hash: "sha256:routes_base".to_string(),
        },
        TreeEntryState {
            artifact_id: "artifact_package_json".to_string(),
            path: "package.json".to_string(),
            content_hash: "sha256:package_base".to_string(),
        },
    ]
}

pub fn fixture_auth_revision() -> TopicRevisionRef {
    fixture_revision(
        "topic_auth_nullability",
        "rev_auth_nullability_0001",
        "op_auth_trim_guard_0001",
        "artifact_src_auth_ts",
        "src/auth.ts",
        "sha256:auth_base",
        "sha256:auth_trim_guard",
        "ctx_agent_a_gen_0001",
        Vec::new(),
    )
}

pub fn fixture_profile_revision() -> TopicRevisionRef {
    fixture_revision(
        "topic_profile_ui",
        "rev_profile_ui_0001",
        "op_profile_write_0001",
        "artifact_src_profile_ts",
        "src/profile.ts",
        "new",
        "sha256:profile_new",
        "ctx_agent_b_gen_0001",
        Vec::new(),
    )
}

pub fn fixture_overlapping_auth_revision() -> TopicRevisionRef {
    fixture_revision(
        "topic_profile_ui",
        "rev_profile_auth_overlap_0001",
        "op_profile_auth_null_guard_0001",
        "artifact_src_auth_ts",
        "src/auth.ts",
        "sha256:auth_base",
        "sha256:auth_null_guard",
        "ctx_agent_b_gen_0001",
        Vec::new(),
    )
}

pub fn fixture_profile_revision_missing_auth_dependency() -> TopicRevisionRef {
    fixture_revision(
        "topic_profile_ui",
        "rev_profile_ui_0002",
        "op_profile_write_0002",
        "artifact_src_profile_ts",
        "src/profile.ts",
        "new",
        "sha256:profile_new_depends_on_auth",
        "ctx_agent_b_gen_0002",
        vec!["rev_auth_nullability_0001".to_string()],
    )
}

fn fixture_revision(
    topic_id: &str,
    revision_id: &str,
    operation_id: &str,
    artifact_id: &str,
    path: &str,
    base_content_hash: &str,
    result_content_hash: &str,
    authored_context_id: &str,
    dependency_revision_ids: Vec<String>,
) -> TopicRevisionRef {
    TopicRevisionRef {
        topic_id: topic_id.to_string(),
        revision_id: revision_id.to_string(),
        operation: OperationRef {
            operation_transaction_id: operation_id.to_string(),
            topic_id: topic_id.to_string(),
            topic_revision_id: revision_id.to_string(),
            artifact_id: artifact_id.to_string(),
            path: path.to_string(),
            mutation: if base_content_hash == "new" {
                ResolverMutationKind::Write
            } else {
                ResolverMutationKind::Patch
            },
            base_content_hash: (base_content_hash != "new").then(|| base_content_hash.to_string()),
            result_content_hash: result_content_hash.to_string(),
            authored_context_id: authored_context_id.to_string(),
        },
        dependency_revision_ids,
    }
}

fn normalize_frontier(frontier: &[TopicRevisionSelection]) -> BTreeMap<String, String> {
    frontier
        .iter()
        .map(|selection| (selection.topic_id.clone(), selection.revision_id.clone()))
        .collect()
}

fn close_dependencies(
    resolved_view_id: &str,
    frontier: &BTreeMap<String, String>,
    revision_refs: &BTreeMap<String, TopicRevisionRef>,
) -> (DependencyClosure, Vec<ResolverConflictOrStalenessRecord>) {
    let mut closure = BTreeSet::new();
    let mut records = Vec::new();
    let selected_revision_ids = frontier.values().cloned().collect::<BTreeSet<_>>();
    let selected_topics = frontier.keys().cloned().collect::<BTreeSet<_>>();

    for revision_id in frontier.values() {
        let Some(revision) = revision_refs.get(revision_id) else {
            continue;
        };
        for dependency_revision_id in &revision.dependency_revision_ids {
            closure.insert(dependency_revision_id.clone());
            let Some(dependency) = revision_refs.get(dependency_revision_id) else {
                records.push(missing_dependency_record(
                    resolved_view_id,
                    revision,
                    dependency_revision_id,
                ));
                continue;
            };

            match frontier.get(&dependency.topic_id) {
                Some(selected_revision_id) if selected_revision_id == dependency_revision_id => {}
                Some(selected_revision_id) => records.push(stale_dependency_record(
                    resolved_view_id,
                    revision,
                    dependency,
                    selected_revision_id,
                )),
                None if !selected_topics.contains(&dependency.topic_id) => {
                    records.push(missing_dependency_record(
                        resolved_view_id,
                        revision,
                        dependency_revision_id,
                    ));
                }
                None => {}
            }
        }
    }

    closure.extend(selected_revision_ids);

    (
        DependencyClosure {
            revision_ids: closure.into_iter().collect(),
        },
        records,
    )
}

fn same_artifact_conflicts(
    resolved_view_id: &str,
    selected_revisions: &[&TopicRevisionRef],
) -> Vec<ResolverConflictOrStalenessRecord> {
    let mut by_artifact = BTreeMap::<String, Vec<&TopicRevisionRef>>::new();
    for revision in selected_revisions {
        by_artifact
            .entry(revision.operation.artifact_id.clone())
            .or_default()
            .push(revision);
    }

    by_artifact
        .into_iter()
        .filter_map(|(artifact_id, revisions)| {
            if revisions.len() <= 1 || revisions_form_dependency_chain(&revisions) {
                return None;
            }
            let operations = revisions
                .iter()
                .map(|revision| &revision.operation)
                .collect::<Vec<_>>();

            let operation_ids = operations
                .iter()
                .map(|operation| operation.operation_transaction_id.clone())
                .collect::<Vec<_>>();
            let authored_context_ids = operations
                .iter()
                .map(|operation| operation.authored_context_id.clone())
                .collect::<Vec<_>>();
            let paths = operations
                .iter()
                .map(|operation| operation.path.clone())
                .collect::<BTreeSet<_>>();
            let path_refs = paths
                .iter()
                .map(|path| PathRef {
                    path: path.clone(),
                    path_state: "active".to_string(),
                })
                .collect::<Vec<_>>();
            let candidate_hashes = operations
                .iter()
                .map(|operation| operation.result_content_hash.clone())
                .collect::<Vec<_>>();
            let base_hashes = operations
                .iter()
                .filter_map(|operation| operation.base_content_hash.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let mut candidate_refs = BTreeMap::new();
            candidate_refs.insert("base_content_hashes".to_string(), base_hashes);
            candidate_refs.insert("candidate_hashes".to_string(), candidate_hashes);
            candidate_refs.insert(
                "operation_semantics_version".to_string(),
                vec![FILE_OPERATION_SEMANTICS_VERSION.to_string()],
            );
            candidate_refs.insert(
                "path_policy_id".to_string(),
                vec![POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string()],
            );

            Some(ResolverConflictOrStalenessRecord {
                id: format!("conflict_{}_0001", artifact_id.replace("artifact_", "")),
                kind: ResolverRecordKind::SameArtifactConflict,
                resolved_view_id: resolved_view_id.to_string(),
                artifact_ids: vec![artifact_id],
                path_refs,
                operation_ids,
                authored_context_ids,
                policy_reason:
                    "same artifact operations are not proven commutative under file_ops_v1"
                        .to_string(),
                candidate_refs,
                resolution_operation_id: None,
            })
        })
        .collect()
}

fn revisions_form_dependency_chain(revisions: &[&TopicRevisionRef]) -> bool {
    let mut remaining = revisions.to_vec();
    let mut previous: Option<&TopicRevisionRef> = None;

    while !remaining.is_empty() {
        let candidates = remaining
            .iter()
            .enumerate()
            .filter(|(_, revision)| match previous {
                Some(previous) => {
                    revision
                        .dependency_revision_ids
                        .contains(&previous.revision_id)
                        && revision.operation.base_content_hash.as_deref()
                            == Some(previous.operation.result_content_hash.as_str())
                }
                None => !revision.dependency_revision_ids.iter().any(|dependency| {
                    remaining
                        .iter()
                        .any(|candidate| candidate.revision_id == *dependency)
                }),
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return false;
        }
        previous = Some(remaining.remove(candidates[0]));
    }

    true
}

fn deterministic_operations<'a>(
    repository_id: &str,
    selected_revisions: Vec<&'a TopicRevisionRef>,
) -> Vec<&'a OperationRef> {
    let mut remaining = selected_revisions;
    let mut operations = Vec::new();
    while !remaining.is_empty() {
        let remaining_ids = remaining
            .iter()
            .map(|revision| revision.revision_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut ready = remaining
            .iter()
            .enumerate()
            .filter(|(_, revision)| {
                !revision
                    .dependency_revision_ids
                    .iter()
                    .any(|dependency| remaining_ids.contains(dependency.as_str()))
            })
            .map(|(index, revision)| (index, revision.operation.canonical_order_key(repository_id)))
            .collect::<Vec<_>>();
        ready.sort_by(|left, right| left.1.cmp(&right.1));
        let index = ready.first().map(|(index, _)| *index).unwrap_or(0);
        let revision = remaining.remove(index);
        operations.push(&revision.operation);
    }
    operations
}

fn missing_dependency_record(
    resolved_view_id: &str,
    dependent_revision: &TopicRevisionRef,
    required_revision_id: &str,
) -> ResolverConflictOrStalenessRecord {
    let mut candidate_refs = BTreeMap::new();
    candidate_refs.insert(
        "required_revision_ids".to_string(),
        vec![required_revision_id.to_string()],
    );
    candidate_refs.insert(
        "dependent_revision_ids".to_string(),
        vec![dependent_revision.revision_id.clone()],
    );

    ResolverConflictOrStalenessRecord {
        id: format!("stale_missing_dependency_{}", required_revision_id),
        kind: ResolverRecordKind::MissingDependency,
        resolved_view_id: resolved_view_id.to_string(),
        artifact_ids: Vec::new(),
        path_refs: Vec::new(),
        operation_ids: vec![dependent_revision
            .operation
            .operation_transaction_id
            .clone()],
        authored_context_ids: vec![dependent_revision.operation.authored_context_id.clone()],
        policy_reason: "required dependency revision is not selected in the frontier".to_string(),
        candidate_refs,
        resolution_operation_id: None,
    }
}

fn stale_dependency_record(
    resolved_view_id: &str,
    dependent_revision: &TopicRevisionRef,
    required_revision: &TopicRevisionRef,
    selected_revision_id: &str,
) -> ResolverConflictOrStalenessRecord {
    let mut candidate_refs = BTreeMap::new();
    candidate_refs.insert(
        "required_revision_ids".to_string(),
        vec![required_revision.revision_id.clone()],
    );
    candidate_refs.insert(
        "selected_revision_ids".to_string(),
        vec![selected_revision_id.to_string()],
    );
    candidate_refs.insert(
        "dependent_revision_ids".to_string(),
        vec![dependent_revision.revision_id.clone()],
    );

    ResolverConflictOrStalenessRecord {
        id: format!(
            "stale_dependency_{}_requires_{}",
            dependent_revision.revision_id, required_revision.revision_id
        ),
        kind: ResolverRecordKind::StaleDependency,
        resolved_view_id: resolved_view_id.to_string(),
        artifact_ids: Vec::new(),
        path_refs: Vec::new(),
        operation_ids: vec![dependent_revision
            .operation
            .operation_transaction_id
            .clone()],
        authored_context_ids: vec![dependent_revision.operation.authored_context_id.clone()],
        policy_reason: "selected dependency revision is older than the required revision"
            .to_string(),
        candidate_refs,
        resolution_operation_id: None,
    }
}

fn resolved_view_id(input: &ResolverInputFrontier, frontier: &BTreeMap<String, String>) -> String {
    let mut parts = vec![
        input.repository_id.clone(),
        input.base_checkpoint_ids.join("+"),
        input.operation_semantics_version.clone(),
        input.path_policy_id.clone(),
    ];
    parts.extend(
        frontier
            .iter()
            .map(|(topic_id, revision_id)| format!("{topic_id}@{revision_id}")),
    );
    format!("view_fixture_{}", stable_label(&parts.join("|")))
}

fn tree_hash(repository_id: &str, entries: &BTreeMap<String, TreeEntryState>) -> String {
    if entries.values().all(|entry| {
        matches!(
            entry.content_hash.as_str(),
            "sha256:auth_base" | "sha256:routes_base" | "sha256:package_base"
        )
    }) {
        return FIXTURE_TREE_HASH.to_string();
    }

    let mut parts = vec![repository_id.to_string()];
    parts.extend(entries.values().map(|entry| {
        format!(
            "{}={}@{}",
            entry.path, entry.artifact_id, entry.content_hash
        )
    }));
    format!("tree_fixture_{}", stable_label(&parts.join("|")))
}

fn stable_label(input: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontier_order_independence() {
        let auth = fixture_auth_revision();
        let profile = fixture_profile_revision();
        let left = resolve_fixture_view(
            fixture_resolver_input(vec![
                selection(&auth.topic_id, &auth.revision_id),
                selection(&profile.topic_id, &profile.revision_id),
            ]),
            fixture_base_entries(),
            vec![auth.clone(), profile.clone()],
        );
        let right = resolve_fixture_view(
            fixture_resolver_input(vec![
                selection(&profile.topic_id, &profile.revision_id),
                selection(&auth.topic_id, &auth.revision_id),
            ]),
            fixture_base_entries(),
            vec![profile, auth],
        );

        assert!(left.conflict_free());
        assert_eq!(left.resolver_order, right.resolver_order);
        assert_eq!(left.tree_identity, right.tree_identity);
        assert_eq!(left.resolved_view_id, right.resolved_view_id);
    }

    #[test]
    fn independent_files_conflict_free_tree_identity() {
        let auth = fixture_auth_revision();
        let profile = fixture_profile_revision();

        let result = resolve_fixture_view(
            fixture_resolver_input(vec![
                selection(&auth.topic_id, &auth.revision_id),
                selection(&profile.topic_id, &profile.revision_id),
            ]),
            fixture_base_entries(),
            vec![auth, profile],
        );

        assert!(result.conflict_free());
        assert!(result.conflicts().next().is_none());
        assert_eq!(
            result.tree_entries["src/auth.ts"].content_hash,
            "sha256:auth_trim_guard"
        );
        assert_eq!(
            result.tree_entries["src/profile.ts"].content_hash,
            "sha256:profile_new"
        );
        assert_eq!(
            result
                .tree_identity
                .as_ref()
                .map(|tree| tree.repository_id.as_str()),
            Some(FIXTURE_REPOSITORY_ID)
        );
    }

    #[test]
    fn same_artifact_overlapping_conflict() {
        let auth = fixture_auth_revision();
        let overlap = fixture_overlapping_auth_revision();

        let result = resolve_fixture_view(
            fixture_resolver_input(vec![
                selection(&auth.topic_id, &auth.revision_id),
                selection(&overlap.topic_id, &overlap.revision_id),
            ]),
            fixture_base_entries(),
            vec![auth, overlap],
        );

        assert!(!result.conflict_free());
        assert!(result.tree_identity.is_none());
        assert_eq!(result.records.len(), 1);
        let conflict = &result.records[0];
        assert_eq!(conflict.kind, ResolverRecordKind::SameArtifactConflict);
        assert_eq!(conflict.artifact_ids, vec!["artifact_src_auth_ts"]);
        assert_eq!(
            conflict.operation_ids,
            vec!["op_auth_trim_guard_0001", "op_profile_auth_null_guard_0001"]
        );
    }

    #[test]
    fn dependent_same_artifact_revision_composes_after_its_exact_dependency() {
        let auth = fixture_auth_revision();
        let cleanup = fixture_revision(
            "topic_aaa_cleanup",
            "rev_cleanup_0001",
            "op_cleanup_0001",
            "artifact_src_auth_ts",
            "src/auth.ts",
            "sha256:auth_trim_guard",
            "sha256:auth_clean",
            "ctx_agent_cleanup_gen_0001",
            vec![auth.revision_id.clone()],
        );

        let result = resolve_fixture_view(
            fixture_resolver_input(vec![
                selection(&cleanup.topic_id, &cleanup.revision_id),
                selection(&auth.topic_id, &auth.revision_id),
            ]),
            fixture_base_entries(),
            vec![cleanup, auth],
        );

        assert!(result.conflict_free());
        assert_eq!(
            result.resolver_order.operation_ids,
            vec!["op_auth_trim_guard_0001", "op_cleanup_0001"]
        );
        assert_eq!(
            result.tree_entries["src/auth.ts"].content_hash,
            "sha256:auth_clean"
        );
    }

    #[test]
    fn missing_dependency_staleness() {
        let dependent = fixture_profile_revision_missing_auth_dependency();
        let required = fixture_auth_revision();

        let result = resolve_fixture_view(
            fixture_resolver_input(vec![selection(&dependent.topic_id, &dependent.revision_id)]),
            fixture_base_entries(),
            vec![dependent, required],
        );

        assert!(!result.conflict_free());
        assert!(result.tree_identity.is_none());
        assert_eq!(result.records.len(), 1);
        let staleness = &result.records[0];
        assert_eq!(staleness.kind, ResolverRecordKind::MissingDependency);
        assert_eq!(
            staleness.candidate_refs["required_revision_ids"],
            vec!["rev_auth_nullability_0001"]
        );
    }

    fn selection(topic_id: &str, revision_id: &str) -> TopicRevisionSelection {
        TopicRevisionSelection {
            topic_id: topic_id.to_string(),
            revision_id: revision_id.to_string(),
        }
    }
}
