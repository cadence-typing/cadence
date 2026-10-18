//! Offline notices from the exact resolved target dependency graph.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::dist::command;

/// Archives dependency and runtime notices; missing notice material fails packaging.
pub fn collect(root: &Path, bundle: &Path) -> Result<(), String> {
    let metadata = command(
        "cargo",
        &[
            OsStr::new("metadata"),
            OsStr::new("--format-version=1"),
            OsStr::new("--locked"),
            OsStr::new("--offline"),
            OsStr::new("--filter-platform=x86_64-unknown-linux-musl"),
        ],
        root,
    )?;
    let metadata: Value = serde_json::from_str(&metadata).map_err(|e| e.to_string())?;
    let packages = array(&metadata, "packages")?;
    let app = packages
        .iter()
        .find(|package| package.get("name").and_then(Value::as_str) == Some("cadence"))
        .ok_or("cadence package absent from metadata")?;
    let nodes = metadata.get("resolve").ok_or("resolve graph missing")?;
    let nodes: BTreeMap<&str, &Value> = array(nodes, "nodes")?
        .iter()
        .map(|node| Ok((text(node, "id")?, node)))
        .collect::<Result<_, String>>()?;
    let mut todo = vec![text(app, "id")?];
    let mut included = BTreeSet::new();
    while let Some(id) = todo.pop() {
        if !included.insert(id) {
            continue;
        }
        let node = nodes
            .get(id)
            .ok_or_else(|| format!("dependency graph node missing: {id}"))?;
        for dep in array(node, "deps")? {
            let shipping = array(dep, "dep_kinds")?
                .iter()
                .any(|kind| kind.get("kind").and_then(Value::as_str) != Some("dev"));
            if shipping {
                todo.push(text(dep, "pkg")?);
            }
        }
    }
    let destination = bundle.join("licenses");
    fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    let mut index = String::from(
        "Resolved normal/build dependencies for the musl application target.\nNames, versions and SPDX declarations come from the committed Cargo.lock graph.\n\n",
    );
    for package in packages {
        if !included.contains(text(package, "id")?)
            || package.get("source").is_none_or(Value::is_null)
        {
            continue;
        }
        let name = text(package, "name")?;
        let version = text(package, "version")?;
        let license = package
            .get("license")
            .and_then(Value::as_str)
            .unwrap_or("see archived license file");
        let source = Path::new(text(package, "manifest_path")?)
            .parent()
            .ok_or("manifest has no parent")?;
        let target = destination.join(format!("{name}-{version}"));
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        let mut found = false;
        for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let file_name = entry.file_name();
            let lower = file_name.to_string_lossy().to_ascii_lowercase();
            if lower.starts_with("license")
                || lower.starts_with("copying")
                || lower.starts_with("notice")
                || lower == "unlicense"
            {
                copy_tree(&entry.path(), &target.join(&file_name))?;
                found = true;
            }
        }
        if let Some(file) = package.get("license_file").and_then(Value::as_str) {
            let path = source.join(file);
            let name = path.file_name().ok_or("invalid license file")?;
            copy_tree(&path, &target.join(name))?;
            found = true;
        }
        if !found {
            return Err(format!("no archived license material for {name} {version}"));
        }
        index.push_str(&format!("{name} {version}: {license}\n"));
    }
    fs::write(destination.join("DEPENDENCIES.txt"), index).map_err(|e| e.to_string())?;
    let sysroot = command("rustc", &[OsStr::new("--print=sysroot")], root)?;
    let rust_docs = Path::new(sysroot.trim()).join("share/doc/rust");
    copy_tree(
        &rust_docs.join("COPYRIGHT-library.html"),
        &destination.join("RUST-COPYRIGHT-library.html"),
    )?;
    copy_tree(
        &rust_docs.join("licenses"),
        &destination.join("rust-licenses"),
    )?;
    copy_tree(
        Path::new("/usr/share/doc/musl/copyright"),
        &destination.join("MUSL-COPYRIGHT"),
    )?;
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    let kind = fs::symlink_metadata(source).map_err(|e| format!("{}: {e}", source.display()))?;
    if kind.is_dir() {
        fs::create_dir_all(target).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_tree(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else if kind.is_file() {
        fs::copy(source, target).map_err(|e| e.to_string())?;
    } else {
        return Err(format!(
            "notice source is not a regular file/directory: {}",
            source.display()
        ));
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("metadata {key} string missing"))
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("metadata {key} array missing"))
}
