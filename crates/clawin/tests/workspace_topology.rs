use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use cargo_metadata::{Metadata, MetadataCommand, Node, Package, PackageId};

// Phase 3 topology remains anchored on DIFF-2026-001 and Clawin-owned crate boundaries.

const EXPECTED_MEMBERS: &[&str] = &[
    "clawin",
    "clawin-bootstrap",
    "clawin-commands",
    "clawin-config",
    "clawin-core",
    "clawin-engine",
    "clawin-integrations",
    "clawin-platform",
    "clawin-tools",
    "clawin-ui",
];

#[test]
fn workspace_members_match_phase_one_contract() {
    let metadata = workspace_metadata();
    let members = metadata
        .workspace_members
        .iter()
        .map(|id| package_name(&metadata, id))
        .collect::<BTreeSet<_>>();

    let expected = EXPECTED_MEMBERS
        .iter()
        .copied()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();

    assert_eq!(members, expected);
}

#[test]
fn forbidden_workspace_edges_are_absent() {
    let metadata = workspace_metadata();
    let nodes = node_map(&metadata);

    assert!(depends_on(&metadata, &nodes, "clawin", "clawin-bootstrap"));
    assert!(depends_on(
        &metadata,
        &nodes,
        "clawin-bootstrap",
        "clawin-commands"
    ));
    assert!(depends_on(
        &metadata,
        &nodes,
        "clawin-bootstrap",
        "clawin-engine"
    ));
    assert!(depends_on(
        &metadata,
        &nodes,
        "clawin-bootstrap",
        "clawin-tools"
    ));
    assert!(depends_on(
        &metadata,
        &nodes,
        "clawin-engine",
        "clawin-commands"
    ));
    assert!(depends_on(
        &metadata,
        &nodes,
        "clawin-engine",
        "clawin-tools"
    ));
    assert!(!depends_on(&metadata, &nodes, "clawin", "clawin-ui"));
    assert!(!depends_on(&metadata, &nodes, "clawin-engine", "clawin-ui"));
    assert!(!depends_on(&metadata, &nodes, "clawin-ui", "clawin-config"));
    assert!(!depends_on(
        &metadata,
        &nodes,
        "clawin-platform",
        "clawin-bootstrap"
    ));
    assert!(!depends_on(
        &metadata,
        &nodes,
        "clawin-platform",
        "clawin-engine"
    ));
    assert!(!depends_on(
        &metadata,
        &nodes,
        "clawin-tools",
        "clawin-commands"
    ));
    assert!(!depends_on(
        &metadata,
        &nodes,
        "clawin-commands",
        "clawin-tools"
    ));
}

fn workspace_metadata() -> Metadata {
    MetadataCommand::new()
        .current_dir(workspace_root())
        .exec()
        .expect("workspace metadata should be readable")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root exists")
        .to_path_buf()
}

fn package_name(metadata: &Metadata, package_id: &PackageId) -> String {
    metadata
        .packages
        .iter()
        .find(|pkg| &pkg.id == package_id)
        .map(|pkg| pkg.name.to_string())
        .expect("package should exist")
}

fn node_map(metadata: &Metadata) -> BTreeMap<&str, &Node> {
    let resolve = metadata.resolve.as_ref().expect("resolve graph exists");
    let by_id = package_map(metadata);

    resolve
        .nodes
        .iter()
        .filter_map(|node| {
            let package = by_id.get(&node.id)?;
            Some((package.name.as_ref(), node))
        })
        .collect()
}

fn package_map(metadata: &Metadata) -> BTreeMap<&PackageId, &Package> {
    metadata.packages.iter().map(|pkg| (&pkg.id, pkg)).collect()
}

fn depends_on(
    metadata: &Metadata,
    nodes: &BTreeMap<&str, &Node>,
    package_name: &str,
    dependency_name: &str,
) -> bool {
    let packages = package_map(metadata);
    let node = nodes.get(package_name).expect("node should exist");

    node.deps.iter().any(|dep| {
        packages
            .get(&dep.pkg)
            .map(|pkg| pkg.name.as_ref() == dependency_name)
            .unwrap_or(false)
    })
}
