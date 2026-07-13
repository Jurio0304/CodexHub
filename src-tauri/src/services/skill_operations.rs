use crate::*;

pub(crate) static ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn timestamp_millis() -> u128 {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    micros
        .saturating_mul(1_000_000)
        .saturating_add(u128::from(ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)))
}

pub(crate) fn local_codex_skills_root() -> Result<PathBuf, String> {
    let root = env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| home_dir().map(|path| path.join(".codex")))
        .ok_or_else(|| {
            "Could not resolve CODEX_HOME or the current user home directory.".to_string()
        })?;
    if !root.is_absolute() {
        return Err("CODEX_HOME must resolve to an absolute path.".into());
    }
    Ok(root.join("skills"))
}

pub(crate) fn load_skill_inventory_status(
    state: &AppState,
) -> Result<SkillInventoryStatus, String> {
    let mut status = storage::load_cache_document(&state.paths, "skills-inventory.json")?
        .unwrap_or(SkillInventoryStatus {
            first_host_scan_completed: false,
            local_skill_root: String::new(),
            local_skills: Vec::new(),
            host_inventories: Vec::new(),
        });
    status.local_skill_root = local_codex_skills_root()?.to_string_lossy().into_owned();
    Ok(status)
}

pub(crate) fn save_skill_inventory_status(
    state: &AppState,
    status: &SkillInventoryStatus,
) -> Result<(), String> {
    storage::save_cache_document(&state.paths, "skills-inventory.json", status)
}

pub(crate) fn apply_skill_inventory_to_hosts(state: &AppState) -> Result<(), String> {
    let status = load_skill_inventory_status(state)?;
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    for host in hosts.iter_mut() {
        if let Some(inventory) = status
            .host_inventories
            .iter()
            .find(|item| item.host_alias.eq_ignore_ascii_case(&host.host_alias))
        {
            let count = if inventory.ok {
                inventory.skills.len().min(u16::MAX as usize) as u16
            } else {
                0
            };
            host.skills_exists = Some(inventory.ok && count > 0);
            host.skills_count = Some(count);
            if inventory.ok {
                host.status = HostStatus::Online;
            }
        }
    }
    Ok(())
}

pub(crate) fn normalize_skill_pack(skill: &mut SkillPack) {
    if skill.source_type == "git" {
        skill.source_type = "github".into();
    }
    if skill.added_at.trim().is_empty() {
        skill.added_at = date_label();
    }
    if skill.updated_at.trim().is_empty() {
        skill.updated_at = timestamp_label();
    }
    if skill.about.trim().is_empty() {
        skill.about = skill.description.clone();
    }
    skill
        .applications
        .retain(|application| !application.target_type.trim().is_empty());
}

pub(crate) fn merge_imported_skill(skills: &mut Vec<SkillPack>, mut skill: SkillPack) -> SkillPack {
    if let Some(existing) = skills.iter().find(|item| item.id == skill.id) {
        skill.added_at = if existing.added_at.trim().is_empty() {
            date_label()
        } else {
            existing.added_at.clone()
        };
        if skill.about.trim().is_empty() {
            skill.about = existing.about.clone();
        }
        skill.applications = existing.applications.clone();
    }
    normalize_skill_pack(&mut skill);
    skills.retain(|item| item.id != skill.id);
    skills.push(skill.clone());
    skill
}

pub(crate) fn import_skills_from_path(
    app: &AppHandle,
    state: &AppState,
    path: PathBuf,
    source_type: &str,
    source_override: Option<String>,
) -> Result<SkillImportResult, String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("Could not resolve skill path: {error}"))?;
    if !path.is_dir() {
        return Err(format!("{} is not a directory.", path.display()));
    }

    let candidate_dirs = skill_candidate_dirs(&path)?;
    if candidate_dirs.is_empty() {
        return Err(format!(
            "{} does not contain a SKILL.md file in the root or immediate child directories.",
            path.display()
        ));
    }

    let mut skills = load_skills(app, state)?;
    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    for candidate in candidate_dirs {
        match import_single_skill(state, &candidate, source_type, source_override.as_deref()) {
            Ok(skill) => {
                imported.push(merge_imported_skill(&mut skills, skill));
            }
            Err(error) => skipped.push(format!("{}: {error}", candidate.display())),
        }
    }
    save_skills(app, state, &skills)?;

    let message = if imported.is_empty() {
        format!("No skills imported; {} candidates skipped.", skipped.len())
    } else {
        format!("Imported {} skill(s).", imported.len())
    };
    Ok(SkillImportResult {
        imported,
        skipped,
        message,
    })
}

pub(crate) fn skill_candidate_dirs(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.join("SKILL.md").is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| format!("Failed to read directory: {error}"))? {
        let entry = entry.map_err(|error| format!("Failed to read directory entry: {error}"))?;
        let child = entry.path();
        if child.is_dir() && child.join("SKILL.md").is_file() {
            candidates.push(child);
        }
    }
    candidates.sort();
    Ok(candidates)
}

pub(crate) fn import_single_skill(
    state: &AppState,
    source_dir: &Path,
    source_type: &str,
    source_override: Option<&str>,
) -> Result<SkillPack, String> {
    let skill_md = source_dir.join("SKILL.md");
    let content = fs::read_to_string(&skill_md)
        .map_err(|error| format!("Failed to read {}: {error}", skill_md.display()))?;
    let metadata = parse_skill_metadata(&content, source_dir)?;
    let id = safe_skill_id(&metadata.name)?;
    let managed_root = managed_skills_dir(state);
    fs::create_dir_all(&managed_root).map_err(|error| error.to_string())?;
    let managed_path = managed_root.join(&id);
    if managed_path.exists() {
        fs::remove_dir_all(&managed_path).map_err(|error| {
            format!(
                "Failed to replace existing managed skill {}: {error}",
                managed_path.display()
            )
        })?;
    }
    copy_skill_dir(source_dir, &managed_path)?;
    let description = metadata.description.unwrap_or_default();
    Ok(SkillPack {
        id: id.clone(),
        name: metadata.name,
        version: metadata.version.unwrap_or_default(),
        description: description.clone(),
        about: description,
        source_type: source_type.into(),
        source: source_override
            .map(str::to_string)
            .unwrap_or_else(|| source_dir.to_string_lossy().into_owned()),
        original_path: Some(source_dir.to_string_lossy().into_owned()),
        managed_path: managed_path.to_string_lossy().into_owned(),
        has_skill_md: true,
        skill_count: 1,
        enabled: true,
        added_at: date_label(),
        updated_at: timestamp_label(),
        applications: Vec::new(),
    })
}

pub(crate) struct ParsedSkillMetadata {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) version: Option<String>,
}

pub(crate) fn parse_skill_metadata(
    content: &str,
    source_dir: &Path,
) -> Result<ParsedSkillMetadata, String> {
    if content.trim().is_empty() {
        return Err("SKILL.md is empty.".into());
    }
    let mut name = None;
    let mut description = None;
    let mut version = None;
    if let Some(frontmatter) = frontmatter_block(content) {
        for line in frontmatter.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = unquote_frontmatter_value(value.trim());
            match key.trim() {
                "name" => name = Some(value),
                "description" => description = Some(value),
                "version" => version = Some(value),
                _ => {}
            }
        }
    }
    let fallback_name = source_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("skill")
        .to_string();
    Ok(ParsedSkillMetadata {
        name: name
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(fallback_name),
        description: description.filter(|value| !value.trim().is_empty()),
        version: version.filter(|value| !value.trim().is_empty()),
    })
}

pub(crate) fn frontmatter_block(content: &str) -> Option<&str> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let rest = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))?;
    let delimiter = rest.find("\n---").or_else(|| rest.find("\r\n---"))?;
    Some(&rest[..delimiter])
}

pub(crate) fn unquote_frontmatter_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn safe_skill_id(name: &str) -> Result<String, String> {
    let slug = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        return Err("Skill name must contain at least one ASCII letter or number.".into());
    }
    if slug == "." || slug == ".." {
        return Err("Skill name resolved to an unsafe path.".into());
    }
    Ok(slug)
}

pub(crate) fn validate_remote_skill_dir_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Skill name is required.".into());
    }
    if name == "." || name == ".." {
        return Err("Skill name resolved to an unsafe path.".into());
    }
    if name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        Ok(name.to_string())
    } else {
        Err(
            "Skill name may only contain ASCII letters, numbers, dots, hyphens, and underscores."
                .into(),
        )
    }
}

pub(crate) fn copy_skill_dir(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("Failed to read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let file_name = entry.file_name();
        if file_name.to_string_lossy() == ".git" {
            continue;
        }
        let destination_path = destination.join(file_name);
        let metadata = fs::symlink_metadata(&source_path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            copy_skill_dir(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "Failed to copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

pub(crate) fn run_detect_installed_skills(
    app: &AppHandle,
    state: &AppState,
    include_hosts: bool,
    timeout_ms: Option<u64>,
) -> Result<SkillDetectionResult, String> {
    let local_root = local_codex_skills_root()?;
    let mut skills = load_skills(app, state)?;
    for skill in &mut skills {
        skill
            .applications
            .retain(|application| application.target_type != "local");
    }

    let mut imported_count = 0usize;
    let mut skipped = Vec::new();
    let mut local_inventory_skills = Vec::new();
    if local_root.is_dir() {
        for candidate in installed_skill_candidate_dirs(&local_root)? {
            match import_single_skill(state, &candidate, "local", None) {
                Ok(detected) => {
                    let application = local_skill_application(&candidate, detected.has_skill_md);
                    let detected_id = detected.id.clone();
                    local_inventory_skills.push(RemoteSkill {
                        name: detected_id.clone(),
                        path: candidate.to_string_lossy().into_owned(),
                        has_skill_md: detected.has_skill_md,
                        status: if detected.has_skill_md {
                            "valid".into()
                        } else {
                            "invalid".into()
                        },
                        description: detected.description.clone(),
                    });
                    if let Some(existing) = skills.iter().find(|item| item.id == detected_id) {
                        let mut merged = detected;
                        merged.source_type = existing.source_type.clone();
                        merged.source = existing.source.clone();
                        merged.original_path = existing.original_path.clone();
                        merged.about = if existing.about.trim().is_empty() {
                            merged.about
                        } else {
                            existing.about.clone()
                        };
                        merge_imported_skill(&mut skills, merged);
                    } else {
                        imported_count += 1;
                        merge_imported_skill(&mut skills, detected);
                    }
                    set_skill_application(&mut skills, &detected_id, application);
                }
                Err(error) => skipped.push(format!("{}: {error}", candidate.display())),
            }
        }
    }

    let mut tasks = Vec::new();
    let mut status = load_skill_inventory_status(state)?;
    status.local_skill_root = local_root.to_string_lossy().into_owned();
    local_inventory_skills.sort_by_key(|skill| skill.name.to_ascii_lowercase());
    status.local_skills = local_inventory_skills;
    if include_hosts {
        let hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
        for host in hosts {
            let result = run_remote_skill_list(state, host.host_alias.clone(), timeout_ms)?;
            let ok = matches!(result.task.status, TaskStatus::Success);
            let previous_inventory = status
                .host_inventories
                .iter()
                .find(|item| item.host_alias.eq_ignore_ascii_case(&result.host_alias))
                .cloned();
            let mut next_inventory = HostSkillInventory {
                host_alias: result.host_alias.clone(),
                scanned_at: timestamp_label(),
                ok,
                message: result.task.summary.clone(),
                skills: result.skills.clone(),
            };
            if ok && result.skills.is_empty() {
                if let Some(previous) = previous_inventory
                    .filter(|inventory| inventory.ok && !inventory.skills.is_empty())
                {
                    let previous_count = previous.skills.len().min(u16::MAX as usize) as u16;
                    update_host_skills(state, &result.host_alias, true, previous_count);
                    next_inventory.message = format!(
                        "Latest scan returned no skills; kept previous cached {} skill(s). {}",
                        previous.skills.len(),
                        result.task.summary
                    );
                    next_inventory.skills = previous.skills;
                }
            }
            upsert_host_inventory(&mut status, next_inventory);
            tasks.push(result.task);
        }
        status.first_host_scan_completed = true;
        refresh_host_applications_from_inventory(&mut skills, &status);
    }

    save_skills(app, state, &skills)?;
    save_skill_inventory_status(state, &status)?;
    let message = if include_hosts {
        format!(
            "Detected local skills and scanned {} host(s). Imported {} new local skill(s); {} skipped.",
            tasks.len(),
            imported_count,
            skipped.len()
        )
    } else {
        format!(
            "Detected local Codex skills. Imported {} new local skill(s); {} skipped.",
            imported_count,
            skipped.len()
        )
    };
    Ok(SkillDetectionResult {
        skills,
        status,
        tasks,
        message,
    })
}

pub(crate) fn installed_skill_candidate_dirs(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    if root.join("SKILL.md").is_file() {
        candidates.push(root.to_path_buf());
    }
    if root.is_dir() {
        for entry in fs::read_dir(root)
            .map_err(|error| format!("Failed to read {}: {error}", root.display()))?
        {
            let entry = entry.map_err(|error| format!("Failed to read skill entry: {error}"))?;
            let child = entry.path();
            if !child.is_dir() {
                continue;
            }
            if child.join("SKILL.md").is_file() {
                candidates.push(child);
            } else {
                for nested in fs::read_dir(&child)
                    .map_err(|error| format!("Failed to read {}: {error}", child.display()))?
                {
                    let nested = nested
                        .map_err(|error| format!("Failed to read nested skill entry: {error}"))?;
                    let nested_path = nested.path();
                    if nested_path.is_dir() && nested_path.join("SKILL.md").is_file() {
                        candidates.push(nested_path);
                    }
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    Ok(candidates)
}

pub(crate) fn set_skill_application(
    skills: &mut [SkillPack],
    skill_id: &str,
    application: SkillApplication,
) {
    if let Some(skill) = skills.iter_mut().find(|skill| skill.id == skill_id) {
        skill.applications.retain(|current| {
            current.target_type != application.target_type
                || current.host_alias != application.host_alias
        });
        skill.applications.push(application);
        skill.updated_at = timestamp_label();
    }
}

pub(crate) fn remove_skill_application(
    skills: &mut [SkillPack],
    skill_id: &str,
    request: &SkillTargetRequest,
) {
    if let Some(skill) = skills.iter_mut().find(|skill| skill.id == skill_id) {
        skill.applications.retain(|current| {
            current.target_type != request.target_type || current.host_alias != request.host_alias
        });
        skill.updated_at = timestamp_label();
    }
}

pub(crate) fn local_skill_application(path: &Path, has_skill_md: bool) -> SkillApplication {
    SkillApplication {
        target_type: "local".into(),
        label: "local".into(),
        host_alias: None,
        path: path.to_string_lossy().into_owned(),
        detected_at: timestamp_label(),
        has_skill_md,
    }
}

pub(crate) fn host_skill_application(
    alias: &str,
    path: &str,
    has_skill_md: bool,
) -> SkillApplication {
    SkillApplication {
        target_type: "host".into(),
        label: alias.to_string(),
        host_alias: Some(alias.to_string()),
        path: path.to_string(),
        detected_at: timestamp_label(),
        has_skill_md,
    }
}

pub(crate) fn refresh_host_applications_from_inventory(
    skills: &mut [SkillPack],
    status: &SkillInventoryStatus,
) {
    let scanned_aliases = status
        .host_inventories
        .iter()
        .filter(|inventory| inventory.ok)
        .map(|inventory| inventory.host_alias.clone())
        .collect::<BTreeSet<_>>();
    for skill in skills.iter_mut() {
        skill.applications.retain(|application| {
            application.target_type != "host"
                || application
                    .host_alias
                    .as_ref()
                    .map(|alias| !scanned_aliases.contains(alias))
                    .unwrap_or(true)
        });
    }
    let mut additions = Vec::new();
    for inventory in status
        .host_inventories
        .iter()
        .filter(|inventory| inventory.ok)
    {
        for skill in skills.iter() {
            if let Some(remote) = inventory
                .skills
                .iter()
                .find(|remote| skill_matches_remote(skill, remote))
            {
                additions.push((
                    skill.id.clone(),
                    host_skill_application(
                        &inventory.host_alias,
                        &remote.path,
                        remote.has_skill_md,
                    ),
                ));
            }
        }
    }
    for (skill_id, application) in additions {
        set_skill_application(skills, &skill_id, application);
    }
}

pub(crate) fn skill_matches_remote(skill: &SkillPack, remote: &RemoteSkill) -> bool {
    remote.name.eq_ignore_ascii_case(&skill.id) || remote.name.eq_ignore_ascii_case(&skill.name)
}

pub(crate) fn upsert_host_inventory(
    status: &mut SkillInventoryStatus,
    inventory: HostSkillInventory,
) {
    status
        .host_inventories
        .retain(|item| !item.host_alias.eq_ignore_ascii_case(&inventory.host_alias));
    status.host_inventories.push(inventory);
    status
        .host_inventories
        .sort_by_key(|item| item.host_alias.to_ascii_lowercase());
}

pub(crate) fn update_host_inventory_skill(
    state: &AppState,
    alias: &str,
    skill_name: &str,
    path: &str,
    installed: bool,
    description: Option<&str>,
) -> Result<(), String> {
    let mut status = load_skill_inventory_status(state)?;
    if let Some(inventory) = status
        .host_inventories
        .iter_mut()
        .find(|item| item.host_alias.eq_ignore_ascii_case(alias))
    {
        inventory.scanned_at = timestamp_label();
        inventory.ok = true;
        inventory
            .skills
            .retain(|skill| !skill.name.eq_ignore_ascii_case(skill_name));
        if installed {
            inventory.skills.push(RemoteSkill {
                name: skill_name.to_string(),
                path: path.to_string(),
                has_skill_md: true,
                status: "valid".into(),
                description: description.unwrap_or_default().to_string(),
            });
        }
        inventory
            .skills
            .sort_by_key(|skill| skill.name.to_ascii_lowercase());
    } else if installed {
        status.host_inventories.push(HostSkillInventory {
            host_alias: alias.to_string(),
            scanned_at: timestamp_label(),
            ok: true,
            message: "Updated from skill operation.".into(),
            skills: vec![RemoteSkill {
                name: skill_name.to_string(),
                path: path.to_string(),
                has_skill_md: true,
                status: "valid".into(),
                description: description.unwrap_or_default().to_string(),
            }],
        });
    }
    save_skill_inventory_status(state, &status)
}

pub(crate) fn update_local_inventory_skill(
    state: &AppState,
    skill_name: &str,
    path: &str,
    installed: bool,
    description: Option<&str>,
) -> Result<(), String> {
    let mut status = load_skill_inventory_status(state)?;
    status.local_skill_root = local_codex_skills_root()?.to_string_lossy().into_owned();
    status
        .local_skills
        .retain(|skill| !skill.name.eq_ignore_ascii_case(skill_name));
    if installed {
        status.local_skills.push(RemoteSkill {
            name: skill_name.to_string(),
            path: path.to_string(),
            has_skill_md: true,
            status: "valid".into(),
            description: description.unwrap_or_default().to_string(),
        });
    }
    status
        .local_skills
        .sort_by_key(|skill| skill.name.to_ascii_lowercase());
    save_skill_inventory_status(state, &status)
}

pub(crate) fn resolve_installed_skill_request(
    state: &AppState,
    request: InstalledSkillRequest,
) -> Result<InstalledSkillRequest, String> {
    let skill_name = validate_remote_skill_dir_name(&request.skill_name)?;
    let requested_path = request.path.trim();
    if requested_path.is_empty() {
        return Err("Installed skill path is required.".into());
    }
    let status = load_skill_inventory_status(state)?;
    match request.target_type.as_str() {
        "local" => {
            let Some(installed) = status.local_skills.iter().find(|skill| {
                skill.has_skill_md
                    && skill.name.eq_ignore_ascii_case(&skill_name)
                    && skill.path == requested_path
            }) else {
                return Err(format!(
                    "Installed skill {skill_name} was not found in the local cached inventory."
                ));
            };
            Ok(InstalledSkillRequest {
                target_type: "local".into(),
                host_alias: None,
                skill_name: installed.name.clone(),
                path: installed.path.clone(),
            })
        }
        "host" => {
            let alias = request
                .host_alias
                .as_deref()
                .ok_or_else(|| "Host alias is required.".to_string())
                .and_then(ssh::validate_ssh_alias)?;
            let Some(inventory) = status
                .host_inventories
                .iter()
                .find(|item| item.host_alias.eq_ignore_ascii_case(&alias) && item.ok)
            else {
                return Err(format!(
                    "Host {alias} does not have a usable cached skill inventory."
                ));
            };
            let Some(installed) = inventory.skills.iter().find(|skill| {
                skill.has_skill_md
                    && skill.name.eq_ignore_ascii_case(&skill_name)
                    && skill.path == requested_path
            }) else {
                return Err(format!(
                    "Installed skill {skill_name} was not found in the cached inventory for {alias}."
                ));
            };
            validate_cached_remote_skill_path(&installed.path)?;
            Ok(InstalledSkillRequest {
                target_type: "host".into(),
                host_alias: Some(alias),
                skill_name: installed.name.clone(),
                path: installed.path.clone(),
            })
        }
        _ => Err("Installed skill target type must be local or host.".into()),
    }
}

pub(crate) fn validate_cached_remote_skill_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() || path.contains(char::is_control) {
        return Err("Cached remote skill path is empty or contains control characters.".into());
    }
    if path.split('/').any(|part| part == "..") {
        return Err("Cached remote skill path contains an unsafe parent segment.".into());
    }
    if !path.contains("/.codex/skills/") && !path.contains("/.codex/superpowers/skills/") {
        return Err("Cached remote skill path is outside known Codex skill roots.".into());
    }
    Ok(())
}

pub(crate) fn skill_matches_installed_request(
    skill: &SkillPack,
    request: &InstalledSkillRequest,
) -> bool {
    skill.id.eq_ignore_ascii_case(&request.skill_name)
        || skill.name.eq_ignore_ascii_case(&request.skill_name)
}

pub(crate) fn remove_installed_skill_application(
    skills: &mut [SkillPack],
    request: &InstalledSkillRequest,
) {
    for skill in skills {
        if !skill_matches_installed_request(skill, request) {
            continue;
        }
        skill.applications.retain(|application| {
            application.target_type != request.target_type
                || application.host_alias != request.host_alias
        });
    }
}

pub(crate) fn download_and_import_github_skill(
    app: &AppHandle,
    state: &AppState,
    repo_url: String,
    timeout_ms: Option<u64>,
) -> Result<SkillImportResult, String> {
    if !is_allowed_github_repo_url(&repo_url) {
        return Err("Only https://github.com/... skill repositories are supported in v1.".into());
    }
    let parsed = parse_github_skill_url(&repo_url).expect("validated GitHub skill URL");
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(120_000)));
    let clone_root = skill_clone_cache_dir(state);
    fs::create_dir_all(&clone_root).map_err(|error| error.to_string())?;
    let clone_dir = clone_root.join(format!(
        "{}-{}",
        safe_skill_id(&parsed.source_url).unwrap_or_else(|_| "github-skill".into()),
        timestamp_millis()
    ));
    let mut args = vec!["clone".into(), "--depth".into(), "1".into()];
    if let Some(branch) = &parsed.branch {
        args.push("--branch".into());
        args.push(branch.clone());
    }
    args.push(parsed.clone_url.clone());
    args.push(clone_dir.to_string_lossy().to_string());
    let command = if let Some(branch) = &parsed.branch {
        format!(
            "git clone --depth 1 --branch {branch} {} {}",
            parsed.clone_url,
            path_string(&clone_dir)
        )
    } else {
        format!(
            "git clone --depth 1 {} {}",
            parsed.clone_url,
            path_string(&clone_dir)
        )
    };
    let output = ssh::run_local_process("git", &args, &command, timeout).unwrap_or_else(|error| {
        failed_command_output(command, format!("Could not start git: {error}"))
    });
    if !output.success() {
        log_best_effort("clean Git clone directory", fs::remove_dir_all(&clone_dir));
        return Err(command_detail(&output));
    }
    let import_path = parsed
        .skill_subpath
        .as_ref()
        .map(|subpath| clone_dir.join(subpath))
        .unwrap_or_else(|| clone_dir.clone());
    if !import_path.exists() {
        log_best_effort("clean Git clone directory", fs::remove_dir_all(&clone_dir));
        return Err(format!(
            "GitHub skill path {} was not found after cloning.",
            parsed.display_path()
        ));
    }
    ensure_child_path(&clone_dir, &import_path)?;
    let result = import_skills_from_path(
        app,
        state,
        import_path,
        "github",
        Some(parsed.source_url.clone()),
    );
    if result.is_err() {
        log_best_effort("clean Git clone directory", fs::remove_dir_all(&clone_dir));
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GithubSkillUrl {
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) clone_url: String,
    pub(crate) branch: Option<String>,
    pub(crate) skill_subpath: Option<PathBuf>,
    pub(crate) source_url: String,
}

impl GithubSkillUrl {
    fn display_path(&self) -> String {
        self.skill_subpath
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.repo.clone())
    }
}

pub(crate) fn is_allowed_github_repo_url(url: &str) -> bool {
    parse_github_skill_url(url).is_some()
}

pub(crate) fn parse_github_skill_url(url: &str) -> Option<GithubSkillUrl> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.contains(char::is_whitespace) {
        return None;
    }
    if trimmed.contains("..")
        || trimmed.contains('\\')
        || trimmed.contains('"')
        || trimmed.contains('\'')
        || trimmed.contains(char::is_control)
    {
        return None;
    }
    let path = trimmed.strip_prefix("https://github.com/")?;
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    if parts.len() != 2 && !(parts.len() >= 5 && parts[2] == "tree") {
        return None;
    }
    let owner = parts[0].to_string();
    let repo_part = parts[1].to_string();
    let repo = repo_part
        .strip_suffix(".git")
        .unwrap_or(&repo_part)
        .to_string();
    if owner.is_empty()
        || repo.is_empty()
        || !is_safe_github_segment(&owner)
        || !is_safe_github_segment(&repo)
    {
        return None;
    }
    if repo_part.ends_with(".git") && parts.len() != 2 {
        return None;
    }
    let clone_url = format!("https://github.com/{owner}/{repo}.git");
    if parts.len() == 2 {
        return Some(GithubSkillUrl {
            owner,
            repo,
            clone_url,
            branch: None,
            skill_subpath: None,
            source_url: trimmed.to_string(),
        });
    }
    let branch = parts[3].to_string();
    if branch.is_empty() || !is_safe_github_tree_segment(&branch) {
        return None;
    }
    let subpath_parts = parts[4..].to_vec();
    if subpath_parts.is_empty()
        || subpath_parts
            .iter()
            .any(|part| !is_safe_github_tree_segment(part) || *part == "." || *part == "..")
    {
        return None;
    }
    let mut skill_subpath = PathBuf::new();
    for part in subpath_parts {
        skill_subpath.push(part);
    }
    Some(GithubSkillUrl {
        owner,
        repo,
        clone_url,
        branch: Some(branch),
        skill_subpath: Some(skill_subpath),
        source_url: trimmed.to_string(),
    })
}

pub(crate) fn is_safe_github_segment(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

pub(crate) fn is_safe_github_tree_segment(value: &str) -> bool {
    !value.is_empty()
        && !value.contains("..")
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '@' | '%'))
}

pub(crate) fn run_get_skill_targets(
    app: &AppHandle,
    state: &AppState,
    skill_id: String,
    _timeout_ms: Option<u64>,
) -> Result<SkillTargetsResult, String> {
    let skill = find_skill(app, state, &skill_id)?;
    let status = load_skill_inventory_status(state)?;
    let mut targets = Vec::new();
    let local_path =
        local_skill_installed_path(&skill)?.unwrap_or(local_skill_target_path(&skill.id)?);
    let local_cached = status
        .local_skills
        .iter()
        .find(|installed| skill_matches_remote(&skill, installed));
    let local_installed = local_cached.is_some()
        || skill
            .applications
            .iter()
            .any(|application| application.target_type == "local");
    let local_display_path = local_cached
        .map(|installed| installed.path.clone())
        .unwrap_or_else(|| local_path.to_string_lossy().into_owned());
    targets.push(SkillTarget {
        target_type: "local".into(),
        label: "local".into(),
        host_alias: None,
        path: local_display_path,
        installed: local_installed,
        can_install: !local_installed
            && PathBuf::from(&skill.managed_path)
                .join("SKILL.md")
                .is_file(),
        can_uninstall: local_installed,
        status: if local_installed {
            "installed"
        } else {
            "available"
        }
        .into(),
        message: if local_installed {
            "Skill is installed on the local Codex root.".into()
        } else {
            "Skill can be installed to the local Codex root.".into()
        },
    });

    let hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    for host in hosts {
        let inventory = status
            .host_inventories
            .iter()
            .find(|item| item.host_alias.eq_ignore_ascii_case(&host.host_alias));
        let cached_skill = inventory.and_then(|inventory| {
            inventory
                .skills
                .iter()
                .find(|installed| skill_matches_remote(&skill, installed))
        });
        let cache_ok = inventory.map(|item| item.ok).unwrap_or(false);
        let installed = cache_ok && cached_skill.is_some();
        let target_path = cached_skill
            .map(|installed| installed.path.clone())
            .unwrap_or_else(|| format!("~/.codex/skills/{}", skill.id));
        targets.push(SkillTarget {
            target_type: "host".into(),
            label: host.host_alias.clone(),
            host_alias: Some(host.host_alias.clone()),
            path: target_path,
            installed,
            can_install: cache_ok && !installed,
            can_uninstall: installed,
            status: if cache_ok {
                if installed {
                    "installed"
                } else {
                    "available"
                }
            } else {
                "cached-unavailable"
            }
            .into(),
            message: if cache_ok {
                if installed {
                    "Cached: skill is installed on this host.".into()
                } else {
                    "Cached: skill can be installed to this host.".into()
                }
            } else {
                inventory
                    .map(|item| item.message.clone())
                    .filter(|message| !message.trim().is_empty())
                    .unwrap_or_else(|| "Run Detect to refresh this host skill cache.".into())
            },
        });
    }

    Ok(SkillTargetsResult {
        skill_id: skill.id,
        skill_name: skill.name,
        targets,
        tasks: Vec::new(),
        message: "Loaded cached skill targets.".into(),
    })
}

pub(crate) fn run_download_installed_skill(
    app: &AppHandle,
    state: &AppState,
    request: InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<InstalledSkillDownloadResult, String> {
    let request = resolve_installed_skill_request(state, request)?;
    let mut tasks = Vec::new();
    let import_result = if request.target_type == "local" {
        import_skills_from_path(
            app,
            state,
            PathBuf::from(&request.path),
            "local",
            Some(request.path.clone()),
        )?
    } else {
        let (extract_dir, task) = download_remote_installed_skill(state, &request, timeout_ms)?;
        tasks.push(task);
        let alias = request.host_alias.clone().unwrap_or_default();
        import_skills_from_path(
            app,
            state,
            extract_dir,
            "host",
            Some(format!("{alias}:{}", request.path)),
        )?
    };
    let skills = load_skills(app, state)?;
    let status = load_skill_inventory_status(state)?;
    let message = if import_result.imported.is_empty() {
        import_result.message.clone()
    } else {
        format!(
            "Downloaded {} to the local skill library.",
            request.skill_name
        )
    };
    Ok(InstalledSkillDownloadResult {
        imported: import_result.imported,
        skipped: import_result.skipped,
        skills,
        status,
        tasks,
        message,
    })
}

pub(crate) fn download_remote_installed_skill(
    state: &AppState,
    request: &InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<(PathBuf, TaskRun), String> {
    let alias = request
        .host_alias
        .as_deref()
        .ok_or_else(|| "Host alias is required.".to_string())
        .and_then(ssh::validate_ssh_alias)?;
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(120_000)));
    let task_id = format!("task-skill-download-{}", timestamp_millis());
    let host_id = host_id_for_alias(state, &alias);
    let host_name = host_name_for_alias(state, &alias);
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        "Download installed skill",
    )?;
    let cache_root = skill_clone_cache_dir(state).join("installed-downloads");
    fs::create_dir_all(&cache_root).map_err(|error| error.to_string())?;
    let remote_archive = format!("/tmp/codexhub-skill-download-{task_id}.tgz");
    let local_archive = cache_root.join(format!("{}.tgz", task_id));
    let extract_dir = cache_root.join(format!("{task_id}-extract"));
    let script = remote_installed_skill_archive_script(&request.path, &remote_archive);
    let package_output = ssh::run_ssh_script(&alias, &script, timeout).unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} package installed skill {}", request.skill_name),
            error,
        )
    });
    let mut logs = running.logs;
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if package_output.success() {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if package_output.success() {
            "Packaged installed remote skill."
        } else {
            "Failed to package installed remote skill."
        },
        &package_output,
    ));
    if !package_output.success() {
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "Download installed skill",
            TaskStatus::Failed,
            &format!(
                "{} could not be packaged on {alias}: {}",
                request.skill_name,
                command_detail(&package_output)
            ),
            logs,
        );
        record_task(state, task.clone())?;
        return Err(task.summary);
    }

    let download_output = ssh::download_file(&alias, &remote_archive, &local_archive, timeout)
        .unwrap_or_else(|error| {
            failed_command_output(format!("scp {alias}:{remote_archive}"), error)
        });
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if download_output.success() {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if download_output.success() {
            "Downloaded installed skill archive."
        } else {
            "Failed to download installed skill archive."
        },
        &download_output,
    ));
    log_best_effort(
        "clean remote skill download archive",
        ssh::run_ssh_script(
            &alias,
            &format!("rm -f {}", shell_single_quote(&remote_archive)),
            timeout,
        ),
    );
    if !download_output.success() {
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "Download installed skill",
            TaskStatus::Failed,
            &format!(
                "{} could not be downloaded from {alias}: {}",
                request.skill_name,
                command_detail(&download_output)
            ),
            logs,
        );
        record_task(state, task.clone())?;
        return Err(task.summary);
    }

    fs::create_dir_all(&extract_dir).map_err(|error| error.to_string())?;
    if let Err(error) = extract_skill_archive(&local_archive, &extract_dir) {
        logs.push(basic_log(
            &task_id,
            logs.len() + 1,
            TaskLogLevel::Error,
            &format!("Failed to extract installed skill archive: {error}"),
        ));
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "Download installed skill",
            TaskStatus::Failed,
            &format!("{} archive could not be extracted.", request.skill_name),
            logs,
        );
        record_task(state, task.clone())?;
        return Err(task.summary);
    }
    logs.push(basic_log(
        &task_id,
        logs.len() + 1,
        TaskLogLevel::Info,
        "Extracted installed skill archive into local cache.",
    ));
    let task = skill_task(
        &task_id,
        &host_id,
        &host_name,
        "Download installed skill",
        TaskStatus::Success,
        &format!("{} downloaded from {alias}.", request.skill_name),
        logs,
    );
    record_task(state, task.clone())?;
    Ok((extract_dir, task))
}

pub(crate) fn extract_skill_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(archive_path)
        .map_err(|error| format!("Failed to open {}: {error}", archive_path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = TarArchive::new(decoder);
    for entry in archive
        .entries()
        .map_err(|error| format!("Failed to read archive entries: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("Failed to read archive entry: {error}"))?;
        let path = entry
            .path()
            .map_err(|error| format!("Failed to read archive path: {error}"))?
            .to_path_buf();
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err("Downloaded skill archive contains unsafe paths.".into());
        }
        entry
            .unpack_in(destination)
            .map_err(|error| format!("Failed to extract archive entry: {error}"))?;
    }
    Ok(())
}

pub(crate) fn run_uninstall_installed_skill(
    app: &AppHandle,
    state: &AppState,
    request: InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    let request = resolve_installed_skill_request(state, request)?;
    let mut skills = load_skills(app, state)?;
    let (item, task) = if request.target_type == "local" {
        uninstall_installed_local_skill(&request)?
    } else {
        let result = run_remote_installed_skill_delete(state, &request, timeout_ms)?;
        (
            SkillTargetOperationItem {
                target_type: "host".into(),
                label: result.host_alias.clone(),
                host_alias: Some(result.host_alias),
                ok: result.ok,
                message: result.message,
                task: Some(result.task.clone()),
            },
            result.task,
        )
    };
    if request.target_type == "local" {
        record_task(state, task.clone())?;
    }
    if item.ok {
        remove_installed_skill_application(&mut skills, &request);
        if request.target_type == "local" {
            update_local_inventory_skill(state, &request.skill_name, "", false, None)?;
        } else if let Some(alias) = &request.host_alias {
            update_host_inventory_skill(
                state,
                alias,
                &request.skill_name,
                &request.path,
                false,
                None,
            )?;
        }
    }
    save_skills(app, state, &skills)?;
    let ok = item.ok;
    let message = if ok {
        "uninstall-success".into()
    } else {
        "uninstall-partial-failure".into()
    };
    Ok(SkillTargetOperationResult {
        ok,
        skills,
        tasks: vec![task],
        results: vec![item],
        message,
    })
}

pub(crate) fn uninstall_installed_local_skill(
    request: &InstalledSkillRequest,
) -> Result<(SkillTargetOperationItem, TaskRun), String> {
    let root = local_codex_skills_root()?;
    let target = PathBuf::from(&request.path);
    let existed = target.exists();
    if existed {
        ensure_child_path(&root, &target)?;
    } else if let Some(parent) = target.parent() {
        ensure_child_path(&root, parent)?;
    } else {
        return Err("Installed local skill path has no parent directory.".into());
    }
    let ok = if !existed {
        true
    } else if target.is_dir() {
        fs::remove_dir_all(&target).map_err(|error| {
            format!(
                "Failed to remove local installed skill {}: {error}",
                target.display()
            )
        })?;
        true
    } else {
        false
    };
    let message = if ok {
        if existed {
            format!("{} removed from {}.", request.skill_name, target.display())
        } else {
            format!(
                "{} was not present at {}.",
                request.skill_name,
                target.display()
            )
        }
    } else {
        format!(
            "Local installed skill target is not a directory: {}.",
            target.display()
        )
    };
    let task = local_skill_task("Uninstall installed skill", &message, ok);
    Ok((
        SkillTargetOperationItem {
            target_type: "local".into(),
            label: "local".into(),
            host_alias: None,
            ok,
            message,
            task: Some(task.clone()),
        },
        task,
    ))
}

pub(crate) fn run_install_skill_targets(
    app: &AppHandle,
    state: &AppState,
    skill_id: String,
    targets: Vec<SkillTargetRequest>,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    let skill = find_skill(app, state, &skill_id)?;
    let mut skills = load_skills(app, state)?;
    let mut tasks = Vec::new();
    let mut results = Vec::new();
    for request in targets {
        if request.target_type == "local" {
            let (item, application, task) = install_local_skill(&skill)?;
            if item.ok {
                let installed_path = application.path.clone();
                set_skill_application(&mut skills, &skill.id, application);
                update_local_inventory_skill(
                    state,
                    &skill.id,
                    &installed_path,
                    true,
                    Some(&skill.description),
                )?;
            }
            if let Some(task) = task.clone() {
                record_task(state, task.clone())?;
                tasks.push(task);
            }
            results.push(item);
        } else if request.target_type == "host" {
            let Some(alias) = request.host_alias.clone() else {
                results.push(failed_target_item(
                    "host",
                    "unknown",
                    None,
                    "Missing host alias.",
                ));
                continue;
            };
            let result = run_remote_skill_install(
                app,
                state,
                alias.clone(),
                skill.id.clone(),
                RemoteSkillScope::User,
                None,
                SkillConflictPolicy::Skip,
                timeout_ms,
            )?;
            let ok = result.ok && !result.skipped;
            if ok {
                set_skill_application(
                    &mut skills,
                    &skill.id,
                    host_skill_application(&result.host_alias, &result.target_path, true),
                );
                update_host_inventory_skill(
                    state,
                    &result.host_alias,
                    &skill.id,
                    &result.target_path,
                    true,
                    Some(&skill.description),
                )?;
            }
            tasks.push(result.task.clone());
            results.push(SkillTargetOperationItem {
                target_type: "host".into(),
                label: result.host_alias.clone(),
                host_alias: Some(result.host_alias),
                ok,
                message: result.message,
                task: Some(result.task),
            });
        }
    }
    save_skills(app, state, &skills)?;
    let ok = results.iter().all(|result| result.ok);
    let message = if ok {
        "install-success".to_string()
    } else {
        "install-partial-failure".to_string()
    };
    Ok(SkillTargetOperationResult {
        ok,
        skills,
        tasks,
        results,
        message,
    })
}

pub(crate) fn run_uninstall_skill_targets(
    app: &AppHandle,
    state: &AppState,
    skill_id: String,
    targets: Vec<SkillTargetRequest>,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    let skill = find_skill(app, state, &skill_id)?;
    let mut skills = load_skills(app, state)?;
    let mut tasks = Vec::new();
    let mut results = Vec::new();
    for request in targets {
        if request.target_type == "local" {
            let (item, task) = uninstall_local_skill(&skill)?;
            if item.ok {
                remove_skill_application(&mut skills, &skill.id, &request);
                update_local_inventory_skill(state, &skill.id, "", false, None)?;
            }
            if let Some(task) = task.clone() {
                record_task(state, task.clone())?;
                tasks.push(task);
            }
            results.push(item);
        } else if request.target_type == "host" {
            let Some(alias) = request.host_alias.clone() else {
                results.push(failed_target_item(
                    "host",
                    "unknown",
                    None,
                    "Missing host alias.",
                ));
                continue;
            };
            let result = run_remote_skill_delete(
                state,
                alias,
                skill.id.clone(),
                RemoteSkillScope::User,
                None,
                skill.id.clone(),
                timeout_ms,
            )?;
            if result.ok {
                remove_skill_application(&mut skills, &skill.id, &request);
                update_host_inventory_skill(
                    state,
                    &result.host_alias,
                    &skill.id,
                    &result.target_path,
                    false,
                    None,
                )?;
            }
            tasks.push(result.task.clone());
            results.push(SkillTargetOperationItem {
                target_type: "host".into(),
                label: result.host_alias.clone(),
                host_alias: Some(result.host_alias),
                ok: result.ok,
                message: result.message,
                task: Some(result.task),
            });
        }
    }
    save_skills(app, state, &skills)?;
    let ok = results.iter().all(|result| result.ok);
    let message = if ok {
        "uninstall-success".to_string()
    } else {
        "uninstall-partial-failure".to_string()
    };
    Ok(SkillTargetOperationResult {
        ok,
        skills,
        tasks,
        results,
        message,
    })
}

pub(crate) fn run_delete_library_skill(
    app: &AppHandle,
    state: &AppState,
    skill_id: String,
    uninstall_first: bool,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    let skill = find_skill(app, state, &skill_id)?;
    let mut tasks = Vec::new();
    let mut results = Vec::new();
    if uninstall_first {
        let targets = skill
            .applications
            .iter()
            .map(|application| SkillTargetRequest {
                target_type: application.target_type.clone(),
                host_alias: application.host_alias.clone(),
            })
            .collect::<Vec<_>>();
        if !targets.is_empty() {
            let uninstall =
                run_uninstall_skill_targets(app, state, skill.id.clone(), targets, timeout_ms)?;
            tasks.extend(uninstall.tasks);
            results.extend(uninstall.results);
            if !uninstall.ok {
                return Ok(SkillTargetOperationResult {
                    ok: false,
                    skills: uninstall.skills,
                    tasks,
                    results,
                    message: "Delete cancelled because one or more uninstall operations failed."
                        .into(),
                });
            }
        }
    }

    delete_managed_skill_dir(state, &skill)?;
    let mut skills = load_skills(app, state)?;
    skills.retain(|item| item.id != skill.id);
    save_skills(app, state, &skills)?;
    let message = format!("Removed {} from the local skill library.", skill.name);
    Ok(SkillTargetOperationResult {
        ok: true,
        skills,
        tasks,
        results,
        message,
    })
}

pub(crate) fn local_skill_target_path(skill_id: &str) -> Result<PathBuf, String> {
    Ok(local_codex_skills_root()?.join(skill_id))
}

pub(crate) fn local_skill_installed_path(skill: &SkillPack) -> Result<Option<PathBuf>, String> {
    let installed = skill
        .applications
        .iter()
        .find(|application| application.target_type == "local")
        .map(|application| PathBuf::from(&application.path))
        .filter(|path| path.join("SKILL.md").is_file());
    if installed.is_some() {
        return Ok(installed);
    }
    let target = local_skill_target_path(&skill.id)?;
    Ok(target.join("SKILL.md").is_file().then_some(target))
}

pub(crate) fn install_local_skill(
    skill: &SkillPack,
) -> Result<(SkillTargetOperationItem, SkillApplication, Option<TaskRun>), String> {
    let source = PathBuf::from(&skill.managed_path);
    if !source.join("SKILL.md").is_file() {
        return Err(format!(
            "Managed skill {} no longer contains SKILL.md.",
            skill.name
        ));
    }
    let root = local_codex_skills_root()?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("Failed to create {}: {error}", root.display()))?;
    let target = root.join(&skill.id);
    if target.exists() {
        let message = format!("{} already exists at {}.", skill.name, target.display());
        let task = local_skill_task("Install skill", &message, false);
        return Ok((
            SkillTargetOperationItem {
                target_type: "local".into(),
                label: "local".into(),
                host_alias: None,
                ok: false,
                message,
                task: Some(task.clone()),
            },
            local_skill_application(&target, target.join("SKILL.md").is_file()),
            Some(task),
        ));
    }
    copy_skill_dir(&source, &target)?;
    let message = format!("Installed {} to {}.", skill.name, target.display());
    let task = local_skill_task("Install skill", &message, true);
    Ok((
        SkillTargetOperationItem {
            target_type: "local".into(),
            label: "local".into(),
            host_alias: None,
            ok: true,
            message,
            task: Some(task.clone()),
        },
        local_skill_application(&target, true),
        Some(task),
    ))
}

pub(crate) fn uninstall_local_skill(
    skill: &SkillPack,
) -> Result<(SkillTargetOperationItem, Option<TaskRun>), String> {
    let Some(target) = local_skill_installed_path(skill)? else {
        let message = format!("{} is not installed in the local Codex root.", skill.name);
        let task = local_skill_task("Uninstall skill", &message, true);
        return Ok((
            SkillTargetOperationItem {
                target_type: "local".into(),
                label: "local".into(),
                host_alias: None,
                ok: true,
                message,
                task: Some(task.clone()),
            },
            Some(task),
        ));
    };
    let root = local_codex_skills_root()?;
    ensure_child_path(&root, &target)?;
    let backup_root = root.join(".codexhub-backups");
    fs::create_dir_all(&backup_root)
        .map_err(|error| format!("Failed to create {}: {error}", backup_root.display()))?;
    let mut backup = backup_root.join(format!("{}.deleted.{}", skill.id, timestamp_label()));
    if backup.exists() {
        backup = backup_root.join(format!(
            "{}.deleted.{}.{}",
            skill.id,
            timestamp_label(),
            timestamp_millis()
        ));
    }
    fs::rename(&target, &backup).map_err(|error| {
        format!(
            "Failed to move {} to {}: {error}",
            target.display(),
            backup.display()
        )
    })?;
    let message = format!("Moved local {} to backup {}.", skill.name, backup.display());
    let task = local_skill_task("Uninstall skill", &message, true);
    Ok((
        SkillTargetOperationItem {
            target_type: "local".into(),
            label: "local".into(),
            host_alias: None,
            ok: true,
            message,
            task: Some(task.clone()),
        },
        Some(task),
    ))
}

pub(crate) fn delete_managed_skill_dir(state: &AppState, skill: &SkillPack) -> Result<(), String> {
    let managed_root = managed_skills_dir(state);
    let managed_path = PathBuf::from(&skill.managed_path);
    if managed_path.exists() {
        ensure_child_path(&managed_root, &managed_path)?;
        fs::remove_dir_all(&managed_path).map_err(|error| {
            format!(
                "Failed to remove managed skill {}: {error}",
                managed_path.display()
            )
        })?;
    }
    Ok(())
}

pub(crate) fn ensure_child_path(root: &Path, child: &Path) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Could not resolve {}: {error}", root.display()))?;
    let child = child
        .canonicalize()
        .map_err(|error| format!("Could not resolve {}: {error}", child.display()))?;
    if child.starts_with(&root) {
        Ok(())
    } else {
        Err(format!(
            "Refusing to modify {} because it is outside {}.",
            child.display(),
            root.display()
        ))
    }
}

pub(crate) fn local_skill_task(action: &str, summary: &str, ok: bool) -> TaskRun {
    skill_task(
        &format!("task-local-skill-{}", timestamp_millis()),
        "local",
        "Local machine",
        action,
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        summary,
        Vec::new(),
    )
}

pub(crate) fn failed_target_item(
    target_type: &str,
    label: &str,
    host_alias: Option<String>,
    message: &str,
) -> SkillTargetOperationItem {
    SkillTargetOperationItem {
        target_type: target_type.into(),
        label: label.into(),
        host_alias,
        ok: false,
        message: message.into(),
        task: None,
    }
}

pub(crate) fn find_skill(
    app: &AppHandle,
    state: &AppState,
    skill_id: &str,
) -> Result<SkillPack, String> {
    load_skills(app, state)?
        .into_iter()
        .find(|skill| skill.id == skill_id)
        .ok_or_else(|| format!("Skill {skill_id} was not found."))
}

pub(crate) fn write_skill_archive(
    state: &AppState,
    skill: &SkillPack,
    task_id: &str,
) -> Result<PathBuf, String> {
    let source = PathBuf::from(&skill.managed_path);
    if !source.join("SKILL.md").is_file() {
        return Err(format!(
            "Managed skill {} no longer contains SKILL.md.",
            skill.name
        ));
    }
    let dir = state.paths.cache_file("skill-upload");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("{task_id}-{}.tgz", skill.id));
    let file = fs::File::create(&path).map_err(|error| error.to_string())?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = TarBuilder::new(encoder);
    tar.append_dir_all("skill", &source)
        .map_err(|error| format!("Failed to archive skill {}: {error}", skill.name))?;
    tar.finish()
        .map_err(|error| format!("Failed to finish skill archive: {error}"))?;
    Ok(path)
}

pub(crate) fn remote_skill_root(
    scope: &RemoteSkillScope,
    project_path: Option<&str>,
) -> Result<(String, String), String> {
    match scope {
        RemoteSkillScope::User => Ok(("$HOME/.codex/skills".into(), "~/.codex/skills".into())),
        RemoteSkillScope::Project => {
            let project_path = project_path
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    "Project path is required for project-level skill install.".to_string()
                })?;
            if project_path.contains('\n') || project_path.contains('\r') {
                return Err("Project path cannot contain line breaks.".into());
            }
            if let Some(suffix) = project_path.strip_prefix("~/") {
                if suffix.is_empty() {
                    return Err("Project path must include a directory after ~/.".into());
                }
                Ok((
                    format!("$HOME/{}/.codex/skills", shell_single_quote(suffix)),
                    format!("{project_path}/.codex/skills"),
                ))
            } else if project_path.starts_with('/') {
                Ok((
                    format!("{}/.codex/skills", shell_single_quote(project_path)),
                    format!("{project_path}/.codex/skills"),
                ))
            } else {
                Err("Project path must start with / or ~/.".into())
            }
        }
    }
}

pub(crate) fn remote_skill_target_display(
    scope: &RemoteSkillScope,
    project_path: Option<&str>,
    skill_name: &str,
) -> Result<String, String> {
    let (_, root) = remote_skill_root(scope, project_path)?;
    Ok(format!("{root}/{skill_name}"))
}

pub(crate) fn remote_skill_list_script() -> &'static str {
    r#"count=0
extract_skill_description() {
  file="$1/SKILL.md"
  [ -f "$file" ] || return
  awk '
    NR == 1 && $0 == "---" { in_frontmatter=1; next }
    in_frontmatter && $0 == "---" { exit }
    in_frontmatter {
      line=$0
      sub(/\r$/, "", line)
      if (line ~ /^[[:space:]]*description[[:space:]]*:/) {
        sub(/^[[:space:]]*description[[:space:]]*:[[:space:]]*/, "", line)
        gsub(/^["'\''"]|["'\''"]$/, "", line)
        print line
        exit
      }
    }
  ' "$file" | tr '\t\r\n' '   ' | sed 's/  */ /g' | cut -c 1-500
}
emit_skill_dir() {
  dir=$1
  [ -d "$dir" ] || return
  name=${dir##*/}
  description=
  if [ -f "$dir/SKILL.md" ]; then
    status=valid
    has=yes
    description=$(extract_skill_description "$dir")
  else
    status=missing-skill-md
    has=no
  fi
  printf 'CODEXHUB_REMOTE_SKILL\t%s\t%s\t%s\t%s\t%s\n' "$name" "$has" "$status" "$dir" "$description"
  count=$((count + 1))
}
scan_child_dir() {
  dir=$1
  [ -d "$dir" ] || return
  if [ -f "$dir/SKILL.md" ]; then
    emit_skill_dir "$dir"
    return
  fi
  before=$count
  for nested in "$dir"/* "$dir"/.[!.]* "$dir"/..?*; do
    [ -d "$nested" ] || continue
    [ -f "$nested/SKILL.md" ] || continue
    emit_skill_dir "$nested"
  done
  if [ "$count" = "$before" ]; then
    emit_skill_dir "$dir"
  fi
}
scan_root() {
  root=$1
  printf 'CODEXHUB_SKILL_ROOT=%s\n' "$root"
  [ -d "$root" ] || return
  if [ -f "$root/SKILL.md" ]; then
    emit_skill_dir "$root"
  else
    for dir in "$root"/* "$root"/.[!.]* "$root"/..?*; do
      scan_child_dir "$dir"
    done
  fi
}
scan_find_fallback() {
  root=$1
  [ -d "$root" ] || return
  command -v find >/dev/null 2>&1 || return
  find "$root" -mindepth 1 -maxdepth 3 -type f -name SKILL.md 2>/dev/null | while IFS= read -r skill_md; do
    dir=${skill_md%/SKILL.md}
    [ -d "$dir" ] || continue
    emit_skill_dir "$dir"
  done
}
scan_root "$HOME/.codex/skills"
scan_root "$HOME/.codex/superpowers/skills"
if [ "$count" = 0 ]; then
  scan_find_fallback "$HOME/.codex/skills"
  scan_find_fallback "$HOME/.codex/superpowers/skills"
fi
printf 'CODEXHUB_SKILL_COUNT=%s\n' "$count"
"#
}

pub(crate) fn run_remote_skill_list(
    state: &AppState,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<RemoteSkillListResult, String> {
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(30_000)));
    let alias = ssh::validate_ssh_alias(&host_alias)?;
    let task_id = format!("task-skill-list-{}", timestamp_millis());
    let host_id = host_id_for_alias(state, &alias);
    let host_name = host_name_for_alias(state, &alias);
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        "List remote skills",
    )?;
    let check_output = ssh::run_ssh_echo_ok(&alias, timeout)
        .unwrap_or_else(|error| failed_command_output(format!("ssh {alias} echo ok"), error));
    let check_ok = check_output.success() && check_output.stdout.trim() == "ok";
    let mut logs = running.logs;
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if check_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &ssh_check_message(&alias, &check_output, check_ok, timeout),
        &check_output,
    ));
    if !check_ok {
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "List remote skills",
            TaskStatus::Failed,
            "Remote skill list skipped because SSH check failed.",
            logs,
        );
        record_task(state, task.clone())?;
        return Ok(RemoteSkillListResult {
            host_alias: alias,
            root_path: "~/.codex/skills; ~/.codex/superpowers/skills".into(),
            count: 0,
            valid_count: 0,
            invalid_count: 0,
            skills: Vec::new(),
            task,
        });
    }

    let script = remote_skill_list_script();
    let output = ssh::run_ssh_script(&alias, script, timeout).unwrap_or_else(|error| {
        failed_command_output(format!("ssh {alias} list remote skills"), error)
    });
    let ok = output.success();
    let skills = if ok {
        parse_remote_skill_list(&output.stdout)
    } else {
        Vec::new()
    };
    let stdout_line_count = output.stdout.lines().count();
    let remote_marker_count = output
        .stdout
        .lines()
        .filter(|line| line.split_whitespace().next() == Some("CODEXHUB_REMOTE_SKILL"))
        .count();
    let stderr_summary = output
        .stderr
        .trim()
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let list_message = if ok {
        if stderr_summary.is_empty() {
            format!(
                "Listed remote Codex skill roots (~/.codex/skills, ~/.codex/superpowers/skills): stdout {stdout_line_count} line(s), markers {remote_marker_count}, parsed {}.",
                skills.len()
            )
        } else {
            format!(
                "Listed remote Codex skill roots (~/.codex/skills, ~/.codex/superpowers/skills): stdout {stdout_line_count} line(s), markers {remote_marker_count}, parsed {}, stderr: {stderr_summary}",
                skills.len()
            )
        }
    } else {
        "Failed to list remote skills.".to_string()
    };
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &list_message,
        &output,
    ));
    let count = skills.len().min(u16::MAX as usize) as u16;
    let valid_count = skills
        .iter()
        .filter(|skill| skill.has_skill_md)
        .count()
        .min(u16::MAX as usize) as u16;
    let invalid_count = count.saturating_sub(valid_count);
    update_host_skills(state, &alias, ok, count);
    let summary = if ok {
        format!(
            "Remote skill list completed for {alias}: {count} skill(s), {valid_count} valid across Codex skill roots."
        )
    } else {
        format!(
            "Remote skill list failed for {alias}: {}",
            command_detail(&output)
        )
    };
    let task = skill_task(
        &task_id,
        &host_id,
        &host_name,
        "List remote skills",
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &summary,
        logs,
    );
    record_task(state, task.clone())?;
    Ok(RemoteSkillListResult {
        host_alias: alias,
        root_path: "~/.codex/skills; ~/.codex/superpowers/skills".into(),
        count,
        valid_count,
        invalid_count,
        skills,
        task,
    })
}

pub(crate) fn parse_remote_skill_list(stdout: &str) -> Vec<RemoteSkill> {
    let mut seen_paths = BTreeSet::new();
    stdout
        .lines()
        .filter_map(|line| {
            let parsed = if line.starts_with("CODEXHUB_REMOTE_SKILL\t") {
                let parts = line.splitn(6, '\t').collect::<Vec<_>>();
                let (marker, name, has_skill_md, status, path, description) = match parts.as_slice()
                {
                    [marker, name, has_skill_md, status, path, description] => {
                        (*marker, *name, *has_skill_md, *status, *path, *description)
                    }
                    [marker, name, has_skill_md, status, path] => {
                        (*marker, *name, *has_skill_md, *status, *path, "")
                    }
                    _ => return None,
                };
                if marker != "CODEXHUB_REMOTE_SKILL" {
                    return None;
                }
                (
                    name.to_string(),
                    has_skill_md == "yes",
                    status.to_string(),
                    path.to_string(),
                    description.trim().to_string(),
                )
            } else {
                let parts = line.split_whitespace().collect::<Vec<_>>();
                if parts.len() < 5 {
                    return None;
                }
                let marker = parts[0];
                if marker != "CODEXHUB_REMOTE_SKILL" {
                    return None;
                }
                // Task-log redaction collapses tab-delimited marker rows to spaces,
                // so keep the first five structural fields and treat the rest as
                // the optional SKILL.md description.
                (
                    parts[1].to_string(),
                    parts[2] == "yes",
                    parts[3].to_string(),
                    parts[4].to_string(),
                    parts[5..].join(" "),
                )
            };
            let (name, has_skill_md, status, path, description) = parsed;
            if !seen_paths.insert(path.to_ascii_lowercase()) {
                return None;
            }
            Some(RemoteSkill {
                name,
                has_skill_md,
                status,
                path,
                description,
            })
        })
        .collect()
}

pub(crate) fn remote_installed_skill_archive_script(
    remote_path: &str,
    archive_path: &str,
) -> String {
    format!(
        r#"set -u
target={remote_path}
archive={archive_path}
if [ ! -d "$target" ]; then
  printf 'Installed skill target is missing or is not a directory: %s\n' "$target" >&2
  exit 2
fi
if [ ! -f "$target/SKILL.md" ]; then
  printf 'Installed skill target does not contain SKILL.md: %s\n' "$target" >&2
  exit 3
fi
if ! command -v tar >/dev/null 2>&1; then
  printf 'tar is required on the remote host for skill download.\n' >&2
  exit 4
fi
mkdir -p "${{archive%/*}}"
rm -f "$archive"
parent=${{target%/*}}
base=${{target##*/}}
if [ -z "$parent" ] || [ "$parent" = "$target" ] || [ -z "$base" ]; then
  printf 'Installed skill target path is not usable: %s\n' "$target" >&2
  exit 5
fi
tar -czf "$archive" -C "$parent" "$base"
printf 'CODEXHUB_SKILL_ARCHIVE=%s\n' "$archive"
"#,
        remote_path = shell_single_quote(remote_path),
        archive_path = shell_single_quote(archive_path)
    )
}

pub(crate) fn run_remote_skill_install(
    app: &AppHandle,
    state: &AppState,
    host_alias: String,
    skill_id: String,
    scope: RemoteSkillScope,
    project_path: Option<String>,
    conflict_policy: SkillConflictPolicy,
    timeout_ms: Option<u64>,
) -> Result<RemoteSkillInstallResult, String> {
    let skill = find_skill(app, state, &skill_id)?;
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(120_000)));
    let alias = ssh::validate_ssh_alias(&host_alias)?;
    let target_display = remote_skill_target_display(&scope, project_path.as_deref(), &skill.id)?;
    let (root_expr, _) = remote_skill_root(&scope, project_path.as_deref())?;
    let task_id = format!("task-skill-install-{}", timestamp_millis());
    let host_id = host_id_for_alias(state, &alias);
    let host_name = host_name_for_alias(state, &alias);
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        "Install skill",
    )?;
    let mut logs = running.logs;
    let mut next_log = logs.len() + 1;
    let check_output = ssh::run_ssh_echo_ok(&alias, timeout)
        .unwrap_or_else(|error| failed_command_output(format!("ssh {alias} echo ok"), error));
    let check_ok = check_output.success() && check_output.stdout.trim() == "ok";
    logs.push(command_log(
        &task_id,
        next_log,
        if check_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &ssh_check_message(&alias, &check_output, check_ok, timeout),
        &check_output,
    ));
    next_log += 1;
    if !check_ok {
        let summary = "Skill install skipped because SSH check failed.";
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "Install skill",
            TaskStatus::Failed,
            summary,
            logs,
        );
        record_task(state, task.clone())?;
        return Ok(RemoteSkillInstallResult {
            host_alias: alias,
            ok: false,
            skill_id: skill.id,
            skill_name: skill.name,
            scope,
            target_path: target_display,
            backup_path: None,
            skipped: false,
            message: summary.into(),
            task,
        });
    }

    let local_archive = match write_skill_archive(state, &skill, &task_id) {
        Ok(path) => path,
        Err(error) => {
            let output = failed_command_output("create local skill archive".into(), error);
            logs.push(command_log(
                &task_id,
                next_log,
                TaskLogLevel::Error,
                "Could not create local skill archive.",
                &output,
            ));
            let task = skill_task(
                &task_id,
                &host_id,
                &host_name,
                "Install skill",
                TaskStatus::Failed,
                "Skill install failed before upload.",
                logs,
            );
            record_task(state, task.clone())?;
            return Ok(RemoteSkillInstallResult {
                host_alias: alias,
                ok: false,
                skill_id: skill.id,
                skill_name: skill.name,
                scope,
                target_path: target_display,
                backup_path: None,
                skipped: false,
                message: task.summary.clone(),
                task,
            });
        }
    };
    let remote_archive = format!("/tmp/codexhub-skill-{task_id}.tgz");
    let upload_output = ssh::upload_file(&alias, &local_archive, &remote_archive, timeout)
        .unwrap_or_else(|error| failed_command_output(format!("scp {remote_archive}"), error));
    log_best_effort("clean local skill archive", fs::remove_file(&local_archive));
    let upload_ok = upload_output.success();
    logs.push(command_log(
        &task_id,
        next_log,
        if upload_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if upload_ok {
            "Uploaded skill archive to remote staging path."
        } else {
            "Failed to upload skill archive."
        },
        &upload_output,
    ));
    next_log += 1;
    if !upload_ok {
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "Install skill",
            TaskStatus::Failed,
            "Skill install failed during upload; remote skills were not changed.",
            logs,
        );
        record_task(state, task.clone())?;
        return Ok(RemoteSkillInstallResult {
            host_alias: alias,
            ok: false,
            skill_id: skill.id,
            skill_name: skill.name,
            scope,
            target_path: target_display,
            backup_path: None,
            skipped: false,
            message: task.summary.clone(),
            task,
        });
    }

    let script = remote_skill_install_script(
        &remote_archive,
        &root_expr,
        &skill.id,
        &conflict_policy,
        &timestamp_label(),
    );
    let output = ssh::run_ssh_script(&alias, &script, timeout).unwrap_or_else(|error| {
        failed_command_output(format!("ssh {alias} install skill {}", skill.id), error)
    });
    let ok = output.success();
    let skipped = marker_value(&output.stdout, "CODEXHUB_SKILL_SKIPPED").as_deref() == Some("yes");
    let backup_path = marker_value(&output.stdout, "CODEXHUB_SKILL_BACKUP");
    logs.push(command_log(
        &task_id,
        next_log,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if ok {
            "Validated and installed remote skill."
        } else {
            "Failed to validate or install remote skill."
        },
        &output,
    ));
    let status = if ok {
        TaskStatus::Success
    } else {
        TaskStatus::Failed
    };
    let summary = if ok && skipped {
        format!(
            "{} already exists on {}; install skipped.",
            skill.name, alias
        )
    } else if ok {
        let count = marker_value(&output.stdout, "CODEXHUB_SKILL_COUNT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or_else(|| remote_count_after_skill_install(state, &alias));
        update_host_skills(state, &alias, true, count);
        match backup_path.as_deref() {
            Some(path) => format!(
                "{} installed to {} with backup {}.",
                skill.name, target_display, path
            ),
            None => format!("{} installed to {}.", skill.name, target_display),
        }
    } else {
        format!(
            "{} could not be installed to {}; see task logs.",
            skill.name, target_display
        )
    };
    let task = skill_task(
        &task_id,
        &host_id,
        &host_name,
        "Install skill",
        status,
        &summary,
        logs,
    );
    record_task(state, task.clone())?;
    Ok(RemoteSkillInstallResult {
        host_alias: alias,
        ok,
        skill_id: skill.id,
        skill_name: skill.name,
        scope,
        target_path: target_display,
        backup_path,
        skipped,
        message: summary,
        task,
    })
}

pub(crate) fn remote_count_after_skill_install(state: &AppState, alias: &str) -> u16 {
    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
        .and_then(|host| host.skills_count)
        .unwrap_or(0)
        .saturating_add(1)
}

pub(crate) fn run_remote_installed_skill_delete(
    state: &AppState,
    request: &InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<RemoteSkillDeleteResult, String> {
    let alias = request
        .host_alias
        .as_deref()
        .ok_or_else(|| "Host alias is required.".to_string())
        .and_then(ssh::validate_ssh_alias)?;
    validate_cached_remote_skill_path(&request.path)?;
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(30_000)));
    let task_id = format!("task-skill-delete-{}", timestamp_millis());
    let host_id = host_id_for_alias(state, &alias);
    let host_name = host_name_for_alias(state, &alias);
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        "Uninstall installed skill",
    )?;
    let script = remote_installed_skill_delete_script(&request.path);
    let output = ssh::run_ssh_script(&alias, &script, timeout).unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} delete installed skill {}", request.skill_name),
            error,
        )
    });
    let ok = output.success();
    let mut logs = running.logs;
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if ok {
            "Permanently removed remote installed skill directory."
        } else {
            "Failed to permanently remove remote installed skill directory."
        },
        &output,
    ));
    let summary = if ok {
        let count = marker_value(&output.stdout, "CODEXHUB_SKILL_COUNT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or_else(|| remote_count_after_skill_delete(state, &alias));
        update_host_skills(state, &alias, true, count);
        format!("{} removed from {}.", request.skill_name, alias)
    } else {
        format!(
            "{} could not be removed from {}: {}",
            request.skill_name,
            alias,
            command_detail(&output)
        )
    };
    let task = skill_task(
        &task_id,
        &host_id,
        &host_name,
        "Uninstall installed skill",
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &summary,
        logs,
    );
    record_task(state, task.clone())?;
    Ok(RemoteSkillDeleteResult {
        host_alias: alias,
        ok,
        skill_name: request.skill_name.clone(),
        target_path: request.path.clone(),
        backup_path: None,
        message: summary,
        task,
    })
}

pub(crate) fn run_remote_skill_delete(
    state: &AppState,
    host_alias: String,
    skill_name: String,
    scope: RemoteSkillScope,
    project_path: Option<String>,
    confirm_name: String,
    timeout_ms: Option<u64>,
) -> Result<RemoteSkillDeleteResult, String> {
    let skill_name = validate_remote_skill_dir_name(&skill_name)?;
    if confirm_name.trim() != skill_name {
        return Err(format!("Confirmation must exactly match {skill_name}."));
    }
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(30_000)));
    let alias = ssh::validate_ssh_alias(&host_alias)?;
    let target_display = remote_skill_target_display(&scope, project_path.as_deref(), &skill_name)?;
    let (root_expr, _) = remote_skill_root(&scope, project_path.as_deref())?;
    let task_id = format!("task-skill-delete-{}", timestamp_millis());
    let host_id = host_id_for_alias(state, &alias);
    let host_name = host_name_for_alias(state, &alias);
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        "Delete skill",
    )?;
    let script = remote_skill_delete_script(&root_expr, &skill_name, &timestamp_label());
    let output = ssh::run_ssh_script(&alias, &script, timeout).unwrap_or_else(|error| {
        failed_command_output(format!("ssh {alias} delete skill {skill_name}"), error)
    });
    let ok = output.success();
    let backup_path = marker_value(&output.stdout, "CODEXHUB_SKILL_BACKUP");
    let mut logs = running.logs;
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if ok {
            "Permanently removed remote skill directory."
        } else {
            "Failed to delete remote skill."
        },
        &output,
    ));
    let summary = if ok {
        let count = marker_value(&output.stdout, "CODEXHUB_SKILL_COUNT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or_else(|| remote_count_after_skill_delete(state, &alias));
        update_host_skills(state, &alias, true, count);
        match backup_path.as_deref() {
            Some(path) => format!("{skill_name} removed from {alias}; backup at {path}."),
            None if output.stdout.contains("Skill was not present.") => {
                format!("{skill_name} was not present on {alias}.")
            }
            None => format!("{skill_name} permanently removed from {alias}."),
        }
    } else {
        format!(
            "{skill_name} could not be removed from {alias}: {}",
            command_detail(&output)
        )
    };
    let task = skill_task(
        &task_id,
        &host_id,
        &host_name,
        "Delete skill",
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &summary,
        logs,
    );
    record_task(state, task.clone())?;
    Ok(RemoteSkillDeleteResult {
        host_alias: alias,
        ok,
        skill_name,
        target_path: target_display,
        backup_path,
        message: summary,
        task,
    })
}

pub(crate) fn remote_count_after_skill_delete(state: &AppState, alias: &str) -> u16 {
    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
        .and_then(|host| host.skills_count)
        .unwrap_or(1)
        .saturating_sub(1)
}

pub(crate) fn remote_skill_install_script(
    archive_path: &str,
    root_expr: &str,
    skill_name: &str,
    policy: &SkillConflictPolicy,
    timestamp: &str,
) -> String {
    let policy = match policy {
        SkillConflictPolicy::Backup => "backup",
        SkillConflictPolicy::Skip => "skip",
        SkillConflictPolicy::Overwrite => "overwrite",
    };
    format!(
        r#"set -u
archive={archive_path}
root={root_expr}
skill_name={skill_name}
policy={policy}
timestamp={timestamp}
target="$root/$skill_name"
backup="$root/$skill_name.codexhub.bak.$timestamp"
extract_dir="${{TMPDIR:-/tmp}}/codexhub-skill-extract.$$"
stage="$root/.codexhub-stage-$skill_name-$timestamp.$$"
cleanup() {{
  rm -rf "$extract_dir" "$stage"
  rm -f "$archive"
}}
trap cleanup EXIT HUP INT TERM
if [ ! -s "$archive" ]; then
  printf 'Uploaded skill archive is missing or empty: %s\n' "$archive" >&2
  exit 2
fi
if ! command -v tar >/dev/null 2>&1; then
  printf 'tar is required on the remote host for skill install.\n' >&2
  exit 3
fi
if ! tar -tzf "$archive" >/dev/null 2>&1; then
  printf 'Uploaded skill archive is not a readable gzip tarball.\n' >&2
  exit 4
fi
if tar -tzf "$archive" | grep -Eq '(^|/)\.\.(/|$)|^/'; then
  printf 'Uploaded skill archive contains unsafe paths.\n' >&2
  exit 5
fi
rm -rf "$extract_dir" "$stage"
mkdir -p "$extract_dir" "$stage" "$root"
tar -xzf "$archive" -C "$extract_dir"
source_dir="$extract_dir/skill"
if [ ! -f "$source_dir/SKILL.md" ]; then
  printf 'Uploaded skill does not contain SKILL.md at archive root.\n' >&2
  exit 6
fi
cp -R "$source_dir/." "$stage/"
if [ ! -f "$stage/SKILL.md" ]; then
  printf 'Staged skill does not contain SKILL.md after copy.\n' >&2
  exit 7
fi
backup_path=""
skipped=no
if [ -e "$target" ]; then
  case "$policy" in
    skip)
      skipped=yes
      rm -rf "$stage"
      ;;
    backup)
      if [ -e "$backup" ]; then
        backup="$backup.$$"
      fi
      mv "$target" "$backup"
      backup_path="$backup"
      mv "$stage" "$target"
      ;;
    overwrite)
      rm -rf "$target"
      mv "$stage" "$target"
      ;;
    *)
      printf 'Unknown conflict policy: %s\n' "$policy" >&2
      exit 8
      ;;
  esac
else
  mv "$stage" "$target"
fi
printf 'CODEXHUB_SKILL_TARGET=%s\n' "$target"
printf 'CODEXHUB_SKILL_BACKUP=%s\n' "$backup_path"
printf 'CODEXHUB_SKILL_SKIPPED=%s\n' "$skipped"
count=0
for dir in "$root"/*; do
  [ -d "$dir" ] || continue
  count=$((count + 1))
done
printf 'CODEXHUB_SKILL_COUNT=%s\n' "$count"
"#,
        archive_path = shell_single_quote(archive_path),
        root_expr = root_expr,
        skill_name = shell_single_quote(skill_name),
        policy = shell_single_quote(policy),
        timestamp = shell_single_quote(timestamp)
    )
}

pub(crate) fn remote_skill_delete_script(
    root_expr: &str,
    skill_name: &str,
    timestamp: &str,
) -> String {
    format!(
        r#"set -u
root={root_expr}
skill_name={skill_name}
timestamp={timestamp}
target="$root/$skill_name"
if [ ! -e "$target" ]; then
  printf 'CODEXHUB_SKILL_TARGET=%s\n' "$target"
  printf 'CODEXHUB_SKILL_BACKUP=\n'
  printf 'Skill was not present.\n'
  exit 0
fi
if [ ! -d "$target" ]; then
  printf 'Remote skill target exists but is not a directory: %s\n' "$target" >&2
  exit 2
fi
rm -rf "$target"
printf 'CODEXHUB_SKILL_TARGET=%s\n' "$target"
printf 'CODEXHUB_SKILL_BACKUP=\n'
count=0
for dir in "$root"/*; do
  [ -d "$dir" ] || continue
  count=$((count + 1))
done
printf 'CODEXHUB_SKILL_COUNT=%s\n' "$count"
"#,
        root_expr = root_expr,
        skill_name = shell_single_quote(skill_name),
        timestamp = shell_single_quote(timestamp)
    )
}

pub(crate) fn remote_installed_skill_delete_script(remote_path: &str) -> String {
    format!(
        r#"set -u
target={remote_path}
if [ ! -e "$target" ]; then
  printf 'CODEXHUB_SKILL_TARGET=%s\n' "$target"
  printf 'Skill was not present.\n'
  exit 0
fi
if [ ! -d "$target" ]; then
  printf 'Remote skill target exists but is not a directory: %s\n' "$target" >&2
  exit 2
fi
rm -rf "$target"
printf 'CODEXHUB_SKILL_TARGET=%s\n' "$target"
count=0
for root in "$HOME/.codex/skills" "$HOME/.codex/superpowers/skills"; do
  [ -d "$root" ] || continue
  for dir in "$root"/* "$root"/.[!.]* "$root"/..?*; do
    [ -d "$dir" ] || continue
    count=$((count + 1))
  done
done
printf 'CODEXHUB_SKILL_COUNT=%s\n' "$count"
"#,
        remote_path = shell_single_quote(remote_path)
    )
}

pub(crate) fn skill_task(
    task_id: &str,
    host_id: &str,
    host_name: &str,
    action: &str,
    status: TaskStatus,
    summary: &str,
    logs: Vec<TaskLog>,
) -> TaskRun {
    TaskRun {
        id: task_id.to_string(),
        host_id: host_id.to_string(),
        host_name: host_name.to_string(),
        action: action.to_string(),
        status,
        started_at: timestamp_label(),
        ended_at: Some(timestamp_label()),
        summary: summary.to_string(),
        steps: Vec::new(),
        logs,
    }
}

pub(crate) fn update_host_skills(state: &AppState, alias: &str, exists: bool, count: u16) {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
    {
        host.status = HostStatus::Online;
        host.skills_exists = Some(exists);
        host.skills_count = Some(count);
        host.last_seen = "just now".into();
    }
}
