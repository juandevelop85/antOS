#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn test_sha256_known_vectors() {
    // Test vector 1: Empty string
    let empty_hash = crypto::sha256(b"");
    assert_eq!(
        empty_hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );

    // Test vector 2: "abc"
    let abc_hash = crypto::sha256(b"abc");
    assert_eq!(
        abc_hash,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );

    // Test vector 3: "antOS"
    let antos_hash = crypto::sha256(b"antOS");
    assert_eq!(
        antos_hash,
        "ac3489e1fa677fe4e736a91431aeea03af6804f981495992d95367ca0dce399a"
    );
}

#[test]
fn test_parse_recipe_toml() {
    let toml_str = r#"
        [package]
        name = "ripgrep"
        version = "14.1.0"
        description = "Fast line-oriented search tool"
        homepage = "https://github.com/BurntSushi/ripgrep"
        license = "MIT"
        binaries = ["rg"]

        [source]
        url = "https://github.com/BurntSushi/ripgrep/archive/14.1.0.tar.gz"
        sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        signature = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        signer_public_key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

        [build]
        dependencies = ["pcre2"]
        script = "cargo build --release"
    "#;

    let manifest = PackageEngine::parse_recipe(toml_str).expect("Failed to parse recipe");
    assert_eq!(manifest.name, "ripgrep");
    assert_eq!(manifest.version, "14.1.0");
    assert_eq!(manifest.binaries, vec!["rg".to_string()]);
    assert_eq!(manifest.dependencies, vec!["pcre2".to_string()]);
    assert!(manifest.sha256.is_some());
}

#[test]
fn test_parse_ollama_recipe() {
    let manifest = PackageEngine::resolve_manifest("ollama").expect("should resolve ollama recipe");
    assert_eq!(manifest.name, "ollama");
    assert_eq!(manifest.binaries, vec!["ollama".to_string()]);
}

#[test]
fn test_package_lifecycle_install_list_remove_and_rollback() {
    let temp_dir = std::env::temp_dir().join("antos_pkg_test_lifecycle");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. Initial status
    let initial_status = PackageEngine::status(&temp_dir).unwrap();
    assert_eq!(initial_status.current_generation, 0);
    assert_eq!(initial_status.total_packages, 0);

    // 2. Install package 1 (ripgrep)
    let rep1 = PackageEngine::install(&temp_dir, "ripgrep", false).unwrap();
    assert_eq!(rep1.generation, 1);
    assert!(rep1.success);
    assert!(Path::new(&rep1.store_path).exists());

    // Verify active binaries symlinked
    let current_rg = PackageEngine::current_bin_dir(&temp_dir).join("rg");
    assert!(current_rg.exists());

    let list1 = PackageEngine::list(&temp_dir).unwrap();
    assert_eq!(list1.len(), 1);
    assert_eq!(list1[0].name, "ripgrep");

    // 3. Install package 2 (jq)
    let rep2 = PackageEngine::install(&temp_dir, "jq", false).unwrap();
    assert_eq!(rep2.generation, 2);

    let list2 = PackageEngine::list(&temp_dir).unwrap();
    assert_eq!(list2.len(), 2);
    assert!(PackageEngine::current_bin_dir(&temp_dir)
        .join("jq")
        .exists());
    assert!(PackageEngine::current_bin_dir(&temp_dir)
        .join("rg")
        .exists());

    // 4. Verify integrity
    let (all_valid, count, details) = PackageEngine::verify(&temp_dir).unwrap();
    assert!(all_valid);
    assert_eq!(count, 2);
    assert_eq!(details.len(), 2);

    // 5. Rollback to generation 1
    let rollback_rep = PackageEngine::rollback(&temp_dir, None).unwrap();
    assert_eq!(rollback_rep.generation, 1);

    let list_after_rollback = PackageEngine::list(&temp_dir).unwrap();
    assert_eq!(list_after_rollback.len(), 1);
    assert_eq!(list_after_rollback[0].name, "ripgrep");
    assert!(PackageEngine::current_bin_dir(&temp_dir)
        .join("rg")
        .exists());
    assert!(!PackageEngine::current_bin_dir(&temp_dir)
        .join("jq")
        .exists());

    // Note: jq still exists in immutable store without orphans!
    assert!(Path::new(&rep2.store_path).exists());

    // 6. Test remove
    let rep3 = PackageEngine::install(&temp_dir, "curl", false).unwrap();
    assert_eq!(rep3.generation, 2); // new generation on top of generation 1
    assert_eq!(PackageEngine::list(&temp_dir).unwrap().len(), 2);

    let remove_rep = PackageEngine::remove(&temp_dir, "curl").unwrap();
    assert_eq!(remove_rep.generation, 3);
    assert_eq!(PackageEngine::list(&temp_dir).unwrap().len(), 1);
    assert!(!PackageEngine::current_bin_dir(&temp_dir)
        .join("curl")
        .exists());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_parse_gui_recipe_toml() {
    let toml_str = r##"
        [package]
        name = "zed-editor"
        version = "0.140.0"
        description = "High-performance, multiplayer code editor"
        homepage = "https://zed.dev"
        license = "GPL-3.0"
        binaries = ["zed"]
        app_type = "gui"

        [source]
        url = "https://github.com/zed-industries/zed/archive/v0.140.0.tar.gz"
        sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

        [desktop]
        name = "Zed"
        generic_name = "Code Editor"
        comment = "A high-performance code editor"
        exec = "zed %F"
        icon = "zed"
        terminal = false
        categories = ["Development", "TextEditor", "IDE"]
        mime_types = ["text/plain", "text/x-rust"]
        startup_notify = true
        startup_wm_class = "dev.zed.Zed"
        keywords = ["editor", "code", "rust"]

        [[icons]]
        name = "zed"
        theme = "hicolor"
        size = "scalable"
        context = "apps"
        format = "svg"
        data = "<svg viewBox=\"0 0 100 100\"><circle cx=\"50\" cy=\"50\" r=\"40\" fill=\"#3b82f6\"/></svg>"
    "##;

    let manifest = PackageEngine::parse_recipe(toml_str).expect("Failed to parse GUI recipe");
    assert_eq!(manifest.name, "zed-editor");
    assert_eq!(manifest.app_type, PackageAppType::Gui);
    assert!(manifest.desktop_entry.is_some());

    let desktop = manifest.desktop_entry.as_ref().unwrap();
    assert_eq!(desktop.name, "Zed");
    assert_eq!(desktop.generic_name, Some("Code Editor".to_string()));
    assert_eq!(desktop.exec, "zed %F");
    assert_eq!(desktop.icon, Some("zed".to_string()));
    assert!(!desktop.terminal);
    assert!(desktop.categories.contains(&"Development".to_string()));
    assert!(desktop.mime_types.contains(&"text/plain".to_string()));
    assert_eq!(desktop.startup_wm_class, Some("dev.zed.Zed".to_string()));

    assert_eq!(manifest.icons.len(), 1);
    let icon = &manifest.icons[0];
    assert_eq!(icon.resolution, "scalable");
    assert_eq!(icon.format, "svg");
}

#[test]
fn test_desktop_entry_generation_and_validation() {
    let manifest = PackageEngine::resolve_manifest("firefox").expect("should resolve firefox");
    assert!(manifest.desktop_entry.is_some());

    let content = PackageEngine::generate_desktop_entry(&manifest);
    assert!(content.contains("[Desktop Entry]"));
    assert!(content.contains("Type=Application"));
    assert!(content.contains("Name=Firefox"));
    assert!(content.contains("Exec=firefox %u"));
    assert!(content.contains("Icon=firefox"));
    assert!(content.contains("Categories=Network;WebBrowser;"));
    assert!(content.contains("StartupNotify=true"));

    let report = PackageEngine::validate_desktop_entry(&content);
    assert!(
        report.valid,
        "Generated desktop entry must be valid. Errors: {:?}",
        report.errors
    );
    assert!(report.errors.is_empty());

    // Validate invalid desktop entry
    let invalid_content = "[Desktop Entry]\nComment=No name or exec or type\n";
    let invalid_report = PackageEngine::validate_desktop_entry(invalid_content);
    assert!(!invalid_report.valid);
    assert!(invalid_report.errors.iter().any(|e| e.contains("Type")));
    assert!(invalid_report.errors.iter().any(|e| e.contains("Name")));
    assert!(invalid_report.errors.iter().any(|e| e.contains("Exec")));
}

#[test]
fn test_gui_package_lifecycle_install_rollback_and_remove() {
    let temp_dir = std::env::temp_dir().join("antos_pkg_gui_test_lifecycle");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. Install Firefox (GUI)
    let rep1 = PackageEngine::install(&temp_dir, "firefox", false).unwrap();
    assert_eq!(rep1.generation, 1);
    assert!(rep1.success);
    assert_eq!(
        rep1.desktop_entries_linked,
        vec!["firefox.desktop".to_string()]
    );
    assert_eq!(rep1.icons_linked, vec!["firefox.svg".to_string()]);

    // Check symlinks
    let current_apps = PackageEngine::current_applications_dir(&temp_dir);
    let current_icons = PackageEngine::current_icons_dir(&temp_dir).join("hicolor/scalable/apps");
    let firefox_desktop_symlink = current_apps.join("firefox.desktop");
    let firefox_icon_symlink = current_icons.join("firefox.svg");

    assert!(
        firefox_desktop_symlink.exists(),
        "firefox.desktop symlink must exist"
    );
    assert!(
        firefox_icon_symlink.exists(),
        "firefox.svg icon symlink must exist"
    );

    // Verify desktop entry content from symlink
    let desktop_raw = fs::read_to_string(&firefox_desktop_symlink).unwrap();
    assert!(desktop_raw.contains("Name=Firefox"));

    // 2. Query desktop apps
    let apps1 = PackageEngine::list_desktop_apps(&temp_dir).unwrap();
    assert_eq!(apps1.len(), 1);
    assert_eq!(apps1[0].name, "Firefox");
    assert_eq!(apps1[0].exec, "firefox %u");
    assert_eq!(apps1[0].package_name, "firefox");
    assert!(apps1[0].icon_path.is_some());

    // 3. Install Alacritty (GUI)
    let rep2 = PackageEngine::install(&temp_dir, "alacritty", false).unwrap();
    assert_eq!(rep2.generation, 2);
    assert_eq!(
        rep2.desktop_entries_linked,
        vec!["alacritty.desktop".to_string()]
    );

    let apps2 = PackageEngine::list_desktop_apps(&temp_dir).unwrap();
    assert_eq!(apps2.len(), 2);
    assert!(current_apps.join("alacritty.desktop").exists());
    assert!(current_icons.join("alacritty.svg").exists());

    // 4. Rollback to generation 1
    let rollback_rep = PackageEngine::rollback(&temp_dir, None).unwrap();
    assert_eq!(rollback_rep.generation, 1);

    let apps_after_rollback = PackageEngine::list_desktop_apps(&temp_dir).unwrap();
    assert_eq!(apps_after_rollback.len(), 1);
    assert_eq!(apps_after_rollback[0].name, "Firefox");
    assert!(
        !current_apps.join("alacritty.desktop").exists(),
        "Alacritty desktop entry should be removed after rollback"
    );
    assert!(
        !current_icons.join("alacritty.svg").exists(),
        "Alacritty icon should be removed after rollback"
    );
    assert!(current_apps.join("firefox.desktop").exists());

    // 5. Remove Firefox
    let remove_rep = PackageEngine::remove(&temp_dir, "firefox").unwrap();
    assert_eq!(remove_rep.generation, 2);
    let apps_after_remove = PackageEngine::list_desktop_apps(&temp_dir).unwrap();
    assert_eq!(apps_after_remove.len(), 0);
    assert!(!current_apps.join("firefox.desktop").exists());
    assert!(!current_icons.join("firefox.svg").exists());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_all_official_recipes_validation() {
    let all =
        PackageEngine::get_all_official_manifests().expect("should load all official recipes");
    assert!(
        all.len() >= 10,
        "Expected at least 10 official recipes, got {}",
        all.len()
    );

    for manifest in &all {
        assert!(!manifest.name.is_empty(), "Recipe name must not be empty");
        assert!(
            !manifest.version.is_empty(),
            "Recipe version must not be empty"
        );
        assert!(
            manifest.source_url.is_some(),
            "Official recipe {} should have a source url",
            manifest.name
        );
        let sha = manifest
            .sha256
            .as_ref()
            .expect("Official recipe should have a sha256");
        assert_eq!(
            sha.len(),
            64,
            "SHA-256 for {} should be 64 characters",
            manifest.name
        );

        if manifest.app_type == PackageAppType::Gui {
            assert!(
                manifest.desktop_entry.is_some(),
                "GUI package {} must have desktop entry",
                manifest.name
            );
            let desktop_entry_content = PackageEngine::generate_desktop_entry(manifest);
            let report = PackageEngine::validate_desktop_entry(&desktop_entry_content);
            assert!(
                report.valid,
                "Desktop entry for {} must be valid. Errors: {:?}",
                manifest.name, report.errors
            );
        }
    }
}

#[test]
fn test_search_catalog() {
    // Query browsers
    let browsers = PackageEngine::search_catalog("browser").expect("should search catalog");
    assert!(browsers.iter().any(|m| m.name == "firefox"));
    assert!(browsers.iter().any(|m| m.name == "chromium"));

    // Query editors
    let editors = PackageEngine::search_catalog("editor").expect("should search catalog");
    assert!(editors.iter().any(|m| m.name == "vscode"));
    assert!(editors.iter().any(|m| m.name == "zed"));
    assert!(editors.iter().any(|m| m.name == "cursor"));

    // Query tools
    let tools = PackageEngine::search_catalog("postman").expect("should search catalog");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "postman");

    // Query wildcard / all
    let all = PackageEngine::search_catalog("*").expect("should search all");
    assert!(all.len() >= 10);
}
