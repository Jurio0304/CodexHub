fn profile_key_entry(profile_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("CodexHub", &format!("profile:{profile_id}:api_key"))
        .map_err(|error| format!("OS credential store is unavailable: {error}"))
}

pub(crate) trait ProfileCredentialAdapter {
    fn load(&self, profile_id: &str) -> Result<Option<String>, String>;
    fn store(&self, profile_id: &str, value: &str) -> Result<(), String>;
    fn delete(&self, profile_id: &str) -> Result<(), String>;
}

pub(crate) struct OsCredentialAdapter;

impl ProfileCredentialAdapter for OsCredentialAdapter {
    fn load(&self, profile_id: &str) -> Result<Option<String>, String> {
        load_profile_api_key_local(profile_id)
    }

    fn store(&self, profile_id: &str, value: &str) -> Result<(), String> {
        store_profile_api_key_local(profile_id, value)
    }

    fn delete(&self, profile_id: &str) -> Result<(), String> {
        delete_profile_api_key_local(profile_id)
    }
}

pub(crate) fn store_profile_api_key_local(profile_id: &str, api_key: &str) -> Result<(), String> {
    profile_key_entry(profile_id)?
        .set_password(api_key)
        .map_err(|error| format!("Failed to store profile API key in OS credential store: {error}"))
}

pub(crate) fn load_profile_api_key_local(profile_id: &str) -> Result<Option<String>, String> {
    match profile_key_entry(profile_id)?.get_password() {
        Ok(api_key) => Ok(Some(api_key)),
        Err(error) if is_missing_credential_error(&error.to_string()) => Ok(None),
        Err(error) => Err(format!(
            "Failed to read profile API key from OS credential store: {error}"
        )),
    }
}

pub(crate) fn delete_profile_api_key_local(profile_id: &str) -> Result<(), String> {
    match profile_key_entry(profile_id)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(error) if is_missing_credential_error(&error.to_string()) => Ok(()),
        Err(error) => Err(format!(
            "Failed to delete profile API key from OS credential store: {error}"
        )),
    }
}

pub(crate) fn profile_api_key_exists(profile_id: &str) -> Result<bool, String> {
    load_profile_api_key_local(profile_id).map(|value| value.is_some())
}

pub(crate) fn is_missing_credential_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("no entry")
        || lower.contains("not found")
        || lower.contains("no matching entry")
        || lower.contains("could not find")
}
