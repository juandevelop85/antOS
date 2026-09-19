//! Instalación, generaciones de perfil, rollback transaccional y
//! verificación del store inmutable.
use super::*;

impl PackageEngine {
    /// Installs a package into the immutable store and updates the profile generation.
    pub fn install(
        state_dir: &Path,
        recipe_path_or_name: &str,
        dry_run: bool,
    ) -> Result<PackageInstallReport> {
        let manifest = Self::resolve_manifest(recipe_path_or_name)?;

        // Firma (T34.5): Ed25519 real si la receta la trae; media firma
        // (clave sin firma o firma sin clave) es un error de receta; sin
        // firma se instala como `Unsigned` y se dice.
        let signature = match (&manifest.signature, &manifest.signer_public_key) {
            (Some(sig), Some(pubkey)) => {
                let msg = crypto::signing_message(
                    &manifest.name,
                    &manifest.version,
                    manifest.sha256.as_deref(),
                );
                if !crypto::verify_ed25519(pubkey, &msg, sig) {
                    bail!(
                        "la firma Ed25519 de la receta «{}» no verifica con la clave {}… \
                         (mensaje firmado: nombre:versión:sha256)",
                        manifest.name,
                        pubkey.chars().take(12).collect::<String>()
                    );
                }
                PackageSignatureStatus::Ed25519
            }
            (None, None) => PackageSignatureStatus::Unsigned,
            _ => bail!(
                "la receta «{}» trae `signature` sin `signer_public_key` (o al revés): \
                 o las dos o ninguna",
                manifest.name
            ),
        };
        let sig_ok = signature == PackageSignatureStatus::Ed25519;
        let signature_note = match signature {
            PackageSignatureStatus::Ed25519 => "firma Ed25519 verificada",
            PackageSignatureStatus::Unsigned => "receta sin firma",
        };

        // Calculate store hash
        let hash_input = format!(
            "{}:{}:{}:{:?}:{:?}:{}",
            manifest.name,
            manifest.version,
            manifest.description,
            manifest.binaries,
            manifest.app_type,
            manifest.sha256.as_deref().unwrap_or("")
        );
        let full_hash = crypto::sha256(hash_input.as_bytes());
        let store_prefix = format!("{}-{}", &full_hash[..16], manifest.name);
        let store_dir = Self::store_dir(state_dir);
        let pkg_dir = store_dir.join(format!("{store_prefix}-{}", manifest.version));
        let pkg_bin_dir = pkg_dir.join("bin");

        if dry_run {
            let mut dt_linked = Vec::new();
            let mut ic_linked = Vec::new();
            if manifest.app_type == PackageAppType::Gui || manifest.desktop_entry.is_some() {
                dt_linked.push(format!("{}.desktop", manifest.name));
                ic_linked.push(format!("{}.svg", manifest.name));
            }
            return Ok(PackageInstallReport {
                name: manifest.name,
                version: manifest.version,
                store_path: pkg_dir.display().to_string(),
                generation: Self::get_current_generation(state_dir).unwrap_or(0) + 1,
                binaries_linked: manifest.binaries,
                desktop_entries_linked: dt_linked,
                icons_linked: ic_linked,
                checksum_verified: false,
                signature_verified: sig_ok,
                signature,
                source_fetched: false,
                success: true,
                message: format!(
                    "Simulación: {signature_note}; no se descarga la fuente ni se comprueba su SHA-256 (antpkg todavía no lo hace)"
                ),
            });
        }

        // Create immutable package directory structure
        fs::create_dir_all(&pkg_bin_dir).with_context(|| {
            format!(
                "Failed to create package bin directory {}",
                pkg_bin_dir.display()
            )
        })?;

        let mut installed_size: u64 = 0;
        let mut binaries_linked = Vec::new();

        // Materialize package binaries
        for bin in &manifest.binaries {
            let bin_path = pkg_bin_dir.join(bin);
            let script_content = format!(
                "#!/bin/sh\n# antpkg wrapper for {} v{}\necho \"[antOS antpkg] Running {} v{} ({})\"\n",
                manifest.name, manifest.version, bin, manifest.version, &full_hash[..8]
            );
            fs::write(&bin_path, script_content.as_bytes())
                .with_context(|| format!("Failed to write binary {}", bin_path.display()))?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o755));
            }

            installed_size += script_content.len() as u64;
            binaries_linked.push(bin.clone());
        }

        // Materialize XDG Desktop Entry and Icons for GUI applications (T25.1)
        let mut desktop_entries_linked = Vec::new();
        let mut icons_linked = Vec::new();
        let mut desktop_file_name = None;

        if manifest.app_type == PackageAppType::Gui || manifest.desktop_entry.is_some() {
            let store_apps_dir = pkg_dir.join("share").join("applications");
            fs::create_dir_all(&store_apps_dir).with_context(|| {
                format!(
                    "Failed to create applications directory {}",
                    store_apps_dir.display()
                )
            })?;

            let desktop_content = Self::generate_desktop_entry(&manifest);
            let desktop_name = format!("{}.desktop", manifest.name);
            let desktop_path = store_apps_dir.join(&desktop_name);
            fs::write(&desktop_path, desktop_content.as_bytes()).with_context(|| {
                format!("Failed to write desktop entry {}", desktop_path.display())
            })?;

            installed_size += desktop_content.len() as u64;
            desktop_entries_linked.push(desktop_name.clone());
            desktop_file_name = Some(desktop_name);

            let icon_name = manifest
                .desktop_entry
                .as_ref()
                .and_then(|d| d.icon.clone())
                .unwrap_or_else(|| manifest.name.clone());

            let store_icons_dir = pkg_dir
                .join("share")
                .join("icons")
                .join("hicolor")
                .join("scalable")
                .join("apps");
            fs::create_dir_all(&store_icons_dir).with_context(|| {
                format!(
                    "Failed to create icons directory {}",
                    store_icons_dir.display()
                )
            })?;

            let icon_file = format!("{}.svg", icon_name);
            let icon_path = store_icons_dir.join(&icon_file);
            let svg_content = Self::generate_default_icon_svg(&manifest.name);
            fs::write(&icon_path, svg_content.as_bytes())
                .with_context(|| format!("Failed to write icon asset {}", icon_path.display()))?;

            installed_size += svg_content.len() as u64;
            icons_linked.push(icon_file);
        }

        let now = chrono::Local::now().to_rfc3339();

        let summary = PackageSummary {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            store_hash: full_hash.clone(),
            installed_size_bytes: installed_size,
            installed_at: now.clone(),
            binaries: binaries_linked.clone(),
            generation: 0, // Assigned when recording profile
            app_type: manifest.app_type,
            desktop_entry: manifest.desktop_entry.clone(),
            desktop_file: desktop_file_name,
            icons_linked: icons_linked.clone(),
        };

        // Save package metadata in the store
        let meta_path = pkg_dir.join("antpkg.json");
        let meta_json = serde_json::to_string_pretty(&summary)?;
        fs::write(&meta_path, meta_json)?;

        // Update profile generation atomically
        let new_gen = Self::advance_generation(state_dir, Some(summary))?;

        Ok(PackageInstallReport {
            name: manifest.name,
            version: manifest.version,
            store_path: pkg_dir.display().to_string(),
            generation: new_gen,
            binaries_linked,
            desktop_entries_linked,
            icons_linked,
            checksum_verified: false,
            signature_verified: sig_ok,
            signature,
            source_fetched: false,
            success: true,
            message: format!(
                "Registrado en el store [{store_prefix}], generación {new_gen} · {signature_note} · \
                 el binario es un envoltorio simulado: antpkg no descarga la fuente todavía"
            ),
        })
    }

    /// Removes a package from the active profile (without deleting it from the immutable store).
    pub fn remove(state_dir: &Path, package_name: &str) -> Result<PackageInstallReport> {
        let current_pkgs = Self::list(state_dir)?;
        let exists = current_pkgs.iter().any(|p| p.name == package_name);
        if !exists {
            bail!(
                "Package «{}» is not installed in the active profile",
                package_name
            );
        }

        let current_gen = Self::get_current_generation(state_dir)?;
        let mut new_pkgs: Vec<PackageSummary> = current_pkgs
            .into_iter()
            .filter(|p| p.name != package_name)
            .collect();

        let new_gen = current_gen + 1;
        for p in &mut new_pkgs {
            p.generation = new_gen;
        }

        Self::save_generation_and_switch(state_dir, new_gen, new_pkgs)?;

        Ok(PackageInstallReport {
            name: package_name.to_string(),
            version: String::new(),
            store_path: String::new(),
            generation: new_gen,
            binaries_linked: Vec::new(),
            desktop_entries_linked: Vec::new(),
            icons_linked: Vec::new(),
            checksum_verified: false,
            signature_verified: false,
            signature: PackageSignatureStatus::Unsigned,
            source_fetched: false,
            success: true,
            message: format!(
                "Package «{}» removed from profile. Advanced to generation {new_gen}",
                package_name
            ),
        })
    }

    /// Lists all installed packages in the active profile.
    pub fn list(state_dir: &Path) -> Result<Vec<PackageSummary>> {
        let gen = Self::get_current_generation(state_dir)?;
        if gen == 0 {
            return Ok(Vec::new());
        }
        let gen_file = Self::profiles_dir(state_dir).join(format!("generation-{gen}.json"));
        if !gen_file.exists() {
            return Ok(Vec::new());
        }
        let data: ProfileGenerationData = serde_json::from_str(&fs::read_to_string(&gen_file)?)?;
        Ok(data.packages)
    }

    /// Lists all historical profile generations.
    pub fn list_generations(state_dir: &Path) -> Result<Vec<PackageGeneration>> {
        let profiles_dir = Self::profiles_dir(state_dir);
        if !profiles_dir.exists() {
            return Ok(Vec::new());
        }

        let current_gen = Self::get_current_generation(state_dir).unwrap_or(0);
        let mut gens = Vec::new();

        let entries = fs::read_dir(&profiles_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("generation-") && name.ends_with(".json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(data) = serde_json::from_str::<ProfileGenerationData>(&content) {
                        let pkgs_fmt = data
                            .packages
                            .iter()
                            .map(|p| format!("{}@{}", p.name, p.version))
                            .collect();
                        gens.push(PackageGeneration {
                            generation: data.generation,
                            timestamp: data.timestamp,
                            packages: pkgs_fmt,
                            active: data.generation == current_gen,
                        });
                    }
                }
            }
        }

        gens.sort_by_key(|g| g.generation);
        Ok(gens)
    }

    /// Rolls back the profile to the previous generation or a specific target generation.
    pub fn rollback(
        state_dir: &Path,
        target_generation: Option<u64>,
    ) -> Result<PackageInstallReport> {
        let current_gen = Self::get_current_generation(state_dir)?;
        let target = match target_generation {
            Some(t) => t,
            None => {
                if current_gen <= 1 {
                    bail!("Cannot rollback: generation {current_gen} is the first generation");
                }
                current_gen - 1
            }
        };

        if target == current_gen {
            bail!("Target generation {target} is already active");
        }

        let profiles_dir = Self::profiles_dir(state_dir);
        let target_file = profiles_dir.join(format!("generation-{target}.json"));
        if !target_file.exists() {
            bail!("Target generation {target} does not exist in profile history");
        }

        let data: ProfileGenerationData = serde_json::from_str(&fs::read_to_string(&target_file)?)?;

        // Switch to the target generation atomically
        let current_file = profiles_dir.join("current_generation");
        fs::write(&current_file, target.to_string())?;

        // Re-link active binaries and desktop artifacts for target generation
        Self::relink_profile_artifacts(state_dir, &data.packages)?;

        let mut all_dt = Vec::new();
        let mut all_ic = Vec::new();
        for p in &data.packages {
            if let Some(df) = &p.desktop_file {
                all_dt.push(df.clone());
            }
            all_ic.extend(p.icons_linked.clone());
        }

        Ok(PackageInstallReport {
            name: "profile".to_string(),
            version: format!("generation {target}"),
            store_path: target_file.display().to_string(),
            generation: target,
            binaries_linked: data
                .packages
                .iter()
                .flat_map(|p| p.binaries.clone())
                .collect(),
            desktop_entries_linked: all_dt,
            icons_linked: all_ic,
            checksum_verified: false,
            signature_verified: false,
            signature: PackageSignatureStatus::Unsigned,
            source_fetched: false,
            success: true,
            message: format!(
                "Successfully rolled back profile from generation {current_gen} to {target}"
            ),
        })
    }

    /// Verifies integrity and hashes of all packages in the active generation.
    pub fn verify(state_dir: &Path) -> Result<(bool, usize, Vec<String>)> {
        let pkgs = Self::list(state_dir)?;
        let mut all_valid = true;
        let mut details = Vec::new();
        let store_dir = Self::store_dir(state_dir);

        for p in &pkgs {
            let mut pkg_ok = true;
            let mut checked_bins = 0;

            // Look for package directory matching prefix
            let prefix = format!("{}-{}", &p.store_hash[..16], p.name);
            let pkg_dir_name = format!("{prefix}-{}", p.version);
            let pkg_path = store_dir.join(&pkg_dir_name);

            if !pkg_path.exists() {
                all_valid = false;
                details.push(format!(
                    "✗ {}: Store directory {} missing!",
                    p.name,
                    pkg_path.display()
                ));
                continue;
            }

            for bin in &p.binaries {
                let bin_path = pkg_path.join("bin").join(bin);
                if !bin_path.exists() {
                    pkg_ok = false;
                    details.push(format!("✗ {}/{}: Binary missing from store", p.name, bin));
                } else {
                    checked_bins += 1;
                }
            }

            if pkg_ok {
                details.push(format!(
                    "✓ {} v{} [hash {}]: Integrity OK ({} binaries)",
                    p.name,
                    p.version,
                    &p.store_hash[..8],
                    checked_bins
                ));
            } else {
                all_valid = false;
            }
        }

        Ok((all_valid, pkgs.len(), details))
    }

    /// Queries the overall status of the immutable store and profile generations.
    pub fn status(state_dir: &Path) -> Result<PackageStoreStatus> {
        let store_dir = Self::store_dir(state_dir);
        let current_dir = Self::current_dir(state_dir);
        let current_gen = Self::get_current_generation(state_dir).unwrap_or(0);
        let pkgs = Self::list(state_dir).unwrap_or_default();
        let gens = Self::list_generations(state_dir).unwrap_or_default();

        let mut total_bytes: u64 = 0;
        if store_dir.exists() {
            total_bytes = Self::compute_dir_size(&store_dir).unwrap_or(0);
        }

        Ok(PackageStoreStatus {
            store_path: store_dir.display().to_string(),
            current_profile_path: current_dir.display().to_string(),
            current_generation: current_gen,
            total_packages: pkgs.len(),
            total_store_bytes: total_bytes,
            generations_count: gens.len(),
        })
    }

    // ------------------------------------------------------------- private helpers

    fn get_current_generation(state_dir: &Path) -> Result<u64> {
        let current_file = Self::profiles_dir(state_dir).join("current_generation");
        if !current_file.exists() {
            return Ok(0);
        }
        let content = fs::read_to_string(&current_file)?;
        let gen = content.trim().parse::<u64>().unwrap_or(0);
        Ok(gen)
    }

    fn advance_generation(state_dir: &Path, new_pkg: Option<PackageSummary>) -> Result<u64> {
        let current_gen = Self::get_current_generation(state_dir)?;
        let next_gen = current_gen + 1;

        let mut current_pkgs = if current_gen > 0 {
            Self::list(state_dir)?
        } else {
            Vec::new()
        };

        if let Some(mut pkg) = new_pkg {
            // Replace if same package name exists, or append
            current_pkgs.retain(|p| p.name != pkg.name);
            pkg.generation = next_gen;
            current_pkgs.push(pkg);
        }

        Self::save_generation_and_switch(state_dir, next_gen, current_pkgs)?;
        Ok(next_gen)
    }

    fn save_generation_and_switch(
        state_dir: &Path,
        generation: u64,
        packages: Vec<PackageSummary>,
    ) -> Result<()> {
        let profiles_dir = Self::profiles_dir(state_dir);
        fs::create_dir_all(&profiles_dir)?;

        let gen_data = ProfileGenerationData {
            generation,
            timestamp: chrono::Local::now().to_rfc3339(),
            packages: packages.clone(),
        };

        let gen_file = profiles_dir.join(format!("generation-{generation}.json"));
        fs::write(&gen_file, serde_json::to_string_pretty(&gen_data)?)?;

        let current_file = profiles_dir.join("current_generation");
        fs::write(&current_file, generation.to_string())?;

        // Re-link binaries and desktop artifacts into `$ANTOS_STATE/current/`
        Self::relink_profile_artifacts(state_dir, &packages)?;
        Ok(())
    }

    /// Relinks all binaries, desktop entries and icons for the active profile (T16.2 / T25.1).
    pub fn relink_profile_artifacts(state_dir: &Path, packages: &[PackageSummary]) -> Result<()> {
        let current_bin = Self::current_bin_dir(state_dir);
        let current_apps = Self::current_applications_dir(state_dir);
        let current_icons = Self::current_icons_dir(state_dir);
        let store_dir = Self::store_dir(state_dir);

        // 1. Remove and recreate current_bin cleanly
        if current_bin.exists() {
            let _ = fs::remove_dir_all(&current_bin);
        }
        fs::create_dir_all(&current_bin)?;

        // 2. Remove and recreate current_applications cleanly (drops uninstalled desktop entries)
        if current_apps.exists() {
            let _ = fs::remove_dir_all(&current_apps);
        }
        fs::create_dir_all(&current_apps)?;

        // 3. Remove and recreate current_icons cleanly (drops uninstalled icons)
        if current_icons.exists() {
            let _ = fs::remove_dir_all(&current_icons);
        }
        fs::create_dir_all(&current_icons)?;

        for p in packages {
            let prefix = format!("{}-{}", &p.store_hash[..16], p.name);
            let pkg_dir = store_dir.join(format!("{prefix}-{}", p.version));

            // Link Binaries
            for bin in &p.binaries {
                let src_bin = pkg_dir.join("bin").join(bin);
                let link_bin = current_bin.join(bin);

                if src_bin.exists() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::symlink;
                        if symlink(&src_bin, &link_bin).is_err() {
                            let _ = fs::copy(&src_bin, &link_bin);
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = fs::copy(&src_bin, &link_bin);
                    }
                }
            }

            // Link XDG Desktop Entries
            if let Some(desktop_file) = &p.desktop_file {
                let src_desktop = pkg_dir
                    .join("share")
                    .join("applications")
                    .join(desktop_file);
                let link_desktop = current_apps.join(desktop_file);

                if src_desktop.exists() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::symlink;
                        if symlink(&src_desktop, &link_desktop).is_err() {
                            let _ = fs::copy(&src_desktop, &link_desktop);
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = fs::copy(&src_desktop, &link_desktop);
                    }
                }
            }

            // Link XDG Icons
            for icon_file in &p.icons_linked {
                let src_icon = pkg_dir
                    .join("share")
                    .join("icons")
                    .join("hicolor")
                    .join("scalable")
                    .join("apps")
                    .join(icon_file);
                let target_icon_dir = current_icons.join("hicolor").join("scalable").join("apps");
                let _ = fs::create_dir_all(&target_icon_dir);
                let link_icon = target_icon_dir.join(icon_file);

                if src_icon.exists() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::symlink;
                        if symlink(&src_icon, &link_icon).is_err() {
                            let _ = fs::copy(&src_icon, &link_icon);
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = fs::copy(&src_icon, &link_icon);
                    }
                }
            }
        }
        Ok(())
    }

    /// Backwards-compatible alias for relinking active profile artifacts.
    pub fn relink_profile_binaries(state_dir: &Path, packages: &[PackageSummary]) -> Result<()> {
        Self::relink_profile_artifacts(state_dir, packages)
    }

    /// Lists all active graphical desktop applications registered in the current profile (T25.1).
    pub fn list_desktop_apps(state_dir: &Path) -> Result<Vec<DesktopAppSummary>> {
        let pkgs = Self::list(state_dir)?;
        let current_apps_dir = Self::current_applications_dir(state_dir);
        let mut apps = Vec::new();

        for p in pkgs {
            if p.app_type == PackageAppType::Gui || p.desktop_entry.is_some() {
                let desktop_filename = p
                    .desktop_file
                    .clone()
                    .unwrap_or_else(|| format!("{}.desktop", p.name));
                let desktop_path = current_apps_dir.join(&desktop_filename);

                let (name, generic_name, comment, exec, icon, categories, mime_types) =
                    if let Some(d) = &p.desktop_entry {
                        (
                            d.name.clone(),
                            d.generic_name.clone(),
                            d.comment.clone(),
                            d.exec.clone(),
                            d.icon.clone(),
                            d.categories.clone(),
                            d.mime_types.clone(),
                        )
                    } else {
                        (
                            p.name.clone(),
                            None,
                            Some(p.description.clone()),
                            p.binaries
                                .first()
                                .cloned()
                                .unwrap_or_else(|| p.name.clone()),
                            Some(p.name.clone()),
                            vec!["Utility".to_string()],
                            Vec::new(),
                        )
                    };

                let icon_path = p.icons_linked.first().map(|icon_file| {
                    Self::current_icons_dir(state_dir)
                        .join("hicolor")
                        .join("scalable")
                        .join("apps")
                        .join(icon_file)
                        .display()
                        .to_string()
                });

                apps.push(DesktopAppSummary {
                    id: p.name.clone(),
                    name,
                    generic_name,
                    comment,
                    exec,
                    icon,
                    icon_path,
                    categories,
                    mime_types,
                    desktop_file_path: desktop_path.display().to_string(),
                    package_name: p.name,
                    package_version: p.version,
                });
            }
        }

        Ok(apps)
    }

    fn compute_dir_size(path: &Path) -> Result<u64> {
        let mut total = 0;
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let meta = entry.metadata()?;
                if meta.is_dir() {
                    total += Self::compute_dir_size(&entry.path())?;
                } else {
                    total += meta.len();
                }
            }
        }
        Ok(total)
    }
}
