mod cargo_toml;

use glob::glob;
use std::{
    fs::{self, canonicalize},
    path::PathBuf,
    process::Command,
};

use cargo_toml::{is_workspace, CargoToml, IsWorkspace};

const CARGO_PATH: &str = "cargo";
const PACKAGE_PREFIX: &str = "contracts/";

/// Checks if the given path is a Cargo project. This is needed
/// to filter the glob results of a workspace member like `contracts/*`
/// to exclude things like non-directories.
#[allow(clippy::ptr_arg)]
fn is_cargo_project(path: &PathBuf) -> bool {
    // Should we do other checks in here? E.g. filter out hidden directories
    // or directories without Cargo.toml. This should be in line with what
    // cargo does for wildcard workspace members.
    path.is_dir()
}

pub fn build() {
    let file = fs::read_to_string("Cargo.toml").unwrap();

    match is_workspace(&file).unwrap() {
        IsWorkspace::Yes { members } => {
            println!("Found workspace member entries: {:?}", &members);
            build_workspace(&members);
        }
        IsWorkspace::NoMembers => {
            println!("Cargo.toml contains a workspace key but has no workspace members");
        }
        IsWorkspace::No => build_single(),
    }
}

pub fn build_workspace(workspace_members: &[String]) {
    let mut all_packages = workspace_members
        .iter()
        .map(|member| {
            glob(member)
                .unwrap()
                .map(|path| path.unwrap())
                .filter(is_cargo_project)
        })
        .flatten()
        .collect::<Vec<_>>();

    all_packages.sort();

    println!("Package directories: {:?}", all_packages);

    let contract_packages = all_packages
        .iter()
        .filter(|p| p.starts_with(PACKAGE_PREFIX))
        .collect::<Vec<_>>();

    println!("Contracts to be built: {:?}", contract_packages);

    let build_dir = PathBuf::from("/target/wasm32-unknown-unknown/release");

    for contract in contract_packages {
        println!("Building {:?} ...", contract);
        let cargo_file = fs::read_to_string(contract.join("Cargo.toml")).unwrap();

        let CargoToml { package, .. } = toml::from_str(&cargo_file).unwrap();

        let package = package.expect("package is required in Cargo.toml");
        let name = package.name.replace("-", "_");
        let variants = package
            .metadata
            .unwrap_or_default()
            .build_variants
            .unwrap_or_default();

        if variants.len() > 0 {
            for variant in variants {
                let mut child = Command::new(CARGO_PATH)
                    .args(&[
                        "build",
                        "--target-dir=/target",
                        "--release",
                        "--features",
                        variant.as_str(),
                        "--lib",
                        "--target=wasm32-unknown-unknown",
                        "--locked",
                    ])
                    .env("RUSTFLAGS", "-C link-arg=-s")
                    .current_dir(canonicalize(contract).unwrap())
                    .spawn()
                    .unwrap();
                let error_code = child.wait().unwrap();
                assert!(error_code.success());

                println!("Built variant: {}", variant);

                fs::rename(
                    build_dir.join(format!("{name}.wasm")),
                    build_dir.join(format!("{name}_{variant}.wasm")),
                )
                .unwrap();
            }
        }

        let mut child = Command::new(CARGO_PATH)
            .args(&[
                "build",
                "--target-dir=/target",
                "--release",
                "--lib",
                "--target=wasm32-unknown-unknown",
                "--locked",
            ])
            .env("RUSTFLAGS", "-C link-arg=-s")
            .current_dir(canonicalize(contract).unwrap())
            .spawn()
            .unwrap();
        let error_code = child.wait().unwrap();
        assert!(error_code.success());
    }
}

fn build_single() {
    let project_path = PathBuf::from(".");

    // Linker flag "-s" for stripping (https://github.com/rust-lang/cargo/issues/3483#issuecomment-431209957)
    // Note that shortcuts from .cargo/config are not available in source code packages from crates.io
    let mut child = Command::new(CARGO_PATH)
        .args(&[
            "build",
            "--target-dir=/target",
            "--release",
            "--lib",
            "--target=wasm32-unknown-unknown",
            "--locked",
        ])
        .env("RUSTFLAGS", "-C link-arg=-s")
        .current_dir(canonicalize(project_path).unwrap())
        .spawn()
        .unwrap();
    let error_code = child.wait().unwrap();
    assert!(error_code.success());
}
