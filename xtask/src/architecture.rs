// @author kongweiguang

//! Cargo dependency and source-module architecture boundaries.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::metadata::{self, Package, WorkspaceMetadata};
use crate::quality;
use crate::source::{self, Token};

const DOMAIN_CRATES: &[&str] = &[
    "gmark-config",
    "gmark-i18n",
    "gmark-markdown",
    "gmark-source-tools",
    "gmark-export",
    "gmark-update-core",
];
const LEGACY_UI_FREE_CRATES: &[&str] = &[
    "gmark-document",
    "gmark-paged-document",
    "gmark-recovery-codec",
];
const EXPORT_ALLOWED_WORKSPACE_DEPENDENCIES: &[&str] = &["gmark-markdown", "gmark-source-tools"];
const WINDOW_PLATFORM_PACKAGES: &[&str] = &[
    "cocoa",
    "core-foundation",
    "core-graphics",
    "objc",
    "objc2",
    "raw-window-handle",
    "wayland-client",
    "wayland-protocols",
    "winit",
    "x11rb",
];

struct ModuleBoundary {
    source: &'static str,
    forbidden_targets: &'static [&'static str],
}

const MODULE_BOUNDARIES: &[ModuleBoundary] = &[
    ModuleBoundary {
        source: "components",
        forbidden_targets: &["editor", "app_menu", "large_file"],
    },
    ModuleBoundary {
        source: "config",
        forbidden_targets: &["editor", "components", "app_menu"],
    },
    ModuleBoundary {
        source: "export",
        forbidden_targets: &["editor", "app_menu"],
    },
    ModuleBoundary {
        source: "net",
        forbidden_targets: &["editor", "app_menu"],
    },
    ModuleBoundary {
        source: "theme",
        forbidden_targets: &["editor", "app_menu"],
    },
    ModuleBoundary {
        source: "ui",
        forbidden_targets: &["editor", "app"],
    },
    ModuleBoundary {
        source: "platform",
        forbidden_targets: &["editor", "app"],
    },
    ModuleBoundary {
        source: "adapters",
        forbidden_targets: &["editor", "app"],
    },
    ModuleBoundary {
        source: "document_host",
        forbidden_targets: &["editor", "app"],
    },
];

pub(crate) fn check(root: &Path) -> Result<(), String> {
    let metadata = metadata::load(root)?;
    let mut violations = Vec::new();
    check_domain_dependencies(root, &metadata, &mut violations)?;
    check_export_dependencies(root, &metadata, &mut violations);
    check_module_boundaries(root, &mut violations)?;
    quality::check_source_structure(root, &mut violations)?;
    source::finish("architecture", violations)
}

fn check_domain_dependencies(
    root: &Path,
    metadata: &WorkspaceMetadata,
    violations: &mut Vec<String>,
) -> Result<(), String> {
    for crate_name in DOMAIN_CRATES.iter().chain(LEGACY_UI_FREE_CRATES).copied() {
        let Some(package) = metadata.package(crate_name) else {
            continue;
        };
        for dependency in &package.dependencies {
            if let Some(category) = forbidden_platform_dependency(dependency) {
                violations.push(format!(
                    "{}: domain crate '{}' depends on forbidden {category} package '{dependency}'",
                    source::relative(root, &package.manifest_path),
                    package.name
                ));
            }
        }
        if DOMAIN_CRATES.contains(&crate_name) {
            if package.dependencies.contains("gmark") {
                violations.push(format!(
                    "{}: domain crate '{}' must not depend on main application package 'gmark'",
                    source::relative(root, &package.manifest_path),
                    package.name
                ));
            }
            check_domain_source_paths(root, package, violations)?;
        }
    }
    Ok(())
}

fn check_domain_source_paths(
    root: &Path,
    package: &Package,
    violations: &mut Vec<String>,
) -> Result<(), String> {
    let source_root = package
        .manifest_path
        .parent()
        .map(|directory| directory.join("src"))
        .ok_or_else(|| format!("package '{}' manifest has no parent", package.name))?;
    for path in source::walk_files(&source_root)? {
        if path.extension() != Some(OsStr::new("rs")) {
            continue;
        }
        let tokens = source::rust_tokens(&source::read_text(&path)?);
        if let Some(forbidden) = forbidden_domain_source_path(&tokens) {
            violations.push(format!(
                "{}: domain crate '{}' references forbidden UI/platform path '{forbidden}'",
                source::relative(root, &path),
                package.name
            ));
        }
    }
    Ok(())
}

fn check_export_dependencies(
    root: &Path,
    metadata: &WorkspaceMetadata,
    violations: &mut Vec<String>,
) {
    let Some(package) = metadata.package("gmark-export") else {
        return;
    };
    for dependency in &package.dependencies {
        if metadata.is_workspace_package(dependency)
            && !EXPORT_ALLOWED_WORKSPACE_DEPENDENCIES.contains(&dependency.as_str())
        {
            violations.push(format!(
                "{}: gmark-export may depend on workspace crate '{dependency}' only through gmark-markdown or gmark-source-tools",
                source::relative(root, &package.manifest_path)
            ));
        }
    }
}

fn check_module_boundaries(root: &Path, violations: &mut Vec<String>) -> Result<(), String> {
    for boundary in MODULE_BOUNDARIES {
        for path in source_files_for_module(root, boundary.source)? {
            let tokens = source::rust_tokens(&source::read_text(&path)?);
            let module_path = root_module_path(root, &path).unwrap_or_default();
            for target in boundary.forbidden_targets {
                if references_root_module(&tokens, &module_path, target) {
                    violations.push(format!(
                        "{}: layer '{}' must not depend on root module '{target}'",
                        source::relative(root, &path),
                        boundary.source
                    ));
                }
            }
        }
    }
    Ok(())
}

fn source_files_for_module(root: &Path, module: &str) -> Result<Vec<PathBuf>, String> {
    let source_root = root.join("src");
    let module_file = source_root.join(format!("{module}.rs"));
    let module_directory = source_root.join(module);
    let mut files = Vec::new();
    if module_file.is_file() {
        files.push(module_file);
    }
    for path in source::walk_files(&module_directory)? {
        if path.extension() == Some(OsStr::new("rs")) {
            files.push(path);
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn root_module_path(root: &Path, path: &Path) -> Option<Vec<String>> {
    let source_root = root.join("src");
    let relative = path.strip_prefix(source_root).ok()?;
    let mut components = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let file_name = components.pop()?;
    let stem = file_name.strip_suffix(".rs")?;
    if !matches!(stem, "lib" | "main" | "mod") {
        components.push(stem.to_owned());
    }
    Some(components)
}

fn references_root_module(tokens: &[Token], module_path: &[String], target: &str) -> bool {
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].is("crate")
            && tokens.get(index + 1).is_some_and(|token| token.is("::"))
            && path_starts_with_target(tokens, index + 2, target)
        {
            return true;
        }
        if tokens[index].is("super") {
            let mut cursor = index;
            let mut levels = 0;
            while tokens.get(cursor).is_some_and(|token| token.is("super"))
                && tokens.get(cursor + 1).is_some_and(|token| token.is("::"))
            {
                levels += 1;
                cursor += 2;
            }
            if levels > 0
                && levels <= module_path.len()
                && module_path.len() == levels
                && path_starts_with_target(tokens, cursor, target)
            {
                return true;
            }
        }
        index += 1;
    }
    false
}

fn path_starts_with_target(tokens: &[Token], mut index: usize, target: &str) -> bool {
    if tokens.get(index).is_some_and(|token| token.is(target)) {
        return true;
    }
    if !tokens.get(index).is_some_and(|token| token.is("{")) {
        return false;
    }
    index += 1;
    let mut depth = 1;
    let mut expects_segment = true;
    while let Some(token) = tokens.get(index) {
        if token.is("{") {
            depth += 1;
            expects_segment = depth == 1;
        } else if token.is("}") {
            depth -= 1;
            if depth == 0 {
                return false;
            }
        } else if depth == 1 && token.is(",") {
            expects_segment = true;
        } else if depth == 1 && expects_segment {
            if token.is(target) {
                return true;
            }
            expects_segment = false;
        }
        index += 1;
    }
    false
}

fn forbidden_platform_dependency(dependency: &str) -> Option<&'static str> {
    if dependency == "gpui" {
        Some("GPUI")
    } else if dependency == "accesskit" || dependency.starts_with("accesskit_") {
        Some("AccessKit")
    } else if is_window_platform_package(dependency) {
        Some("window-platform")
    } else {
        None
    }
}

fn is_window_platform_package(dependency: &str) -> bool {
    WINDOW_PLATFORM_PACKAGES.contains(&dependency)
        || dependency == "windows"
        || dependency.starts_with("windows-")
}

fn forbidden_domain_source_path(tokens: &[Token]) -> Option<String> {
    // Domain crates may use OS-specific filesystem extensions for no-follow,
    // reparse-point, permission, and atomic-I/O guarantees. Process extensions
    // remain forbidden because they cross the pure-domain/side-effect boundary.
    if contains_path(tokens, &["std", "os", "windows", "process"]) {
        return Some("std::os::windows::process".to_owned());
    }
    for dependency in ["gpui", "accesskit"]
        .into_iter()
        .chain(WINDOW_PLATFORM_PACKAGES.iter().copied())
        .chain(["windows", "windows_sys"])
    {
        let source_name = dependency.replace('-', "_");
        if contains_root_path(tokens, source_name.as_str()) {
            return Some(source_name);
        }
    }
    None
}

fn contains_root_path(tokens: &[Token], name: &str) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        token.is(name)
            && tokens.get(index + 1).is_some_and(|token| token.is("::"))
            && (index == 0
                || !tokens
                    .get(index.wrapping_sub(1))
                    .is_some_and(|token| token.is("::"))
                || index == 1)
    })
}

fn contains_path(tokens: &[Token], path: &[&str]) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        token.is(path[0])
            && if path.len() == 1 {
                tokens.get(index + 1).is_some_and(|token| token.is("::"))
            } else {
                path.iter().enumerate().skip(1).all(|(part_index, part)| {
                    tokens
                        .get(index + part_index * 2 - 1)
                        .is_some_and(|token| token.is("::"))
                        && tokens
                            .get(index + part_index * 2)
                            .is_some_and(|token| token.is(part))
                })
            }
    })
}
