use crate::ssh;

const REMOTE_CODEX_RELEASE_CLEANUP_TIMEOUT_MS: u64 = 360_000;

// Every CodexHub path that mutates the remote runtime shares this lock with
// cleanup. The complete owner record is published atomically with a hard link.
pub(crate) const REMOTE_CODEX_RUNTIME_WRITER_LOCK_PRELUDE: &str = r###"set -u
codexhub_runtime_lock_root="$HOME"
codexhub_runtime_lock_path="$codexhub_runtime_lock_root/.codexhub-runtime-cleanup.lock"
codexhub_runtime_lock_proc=/proc
codexhub_runtime_lock_held=no
codexhub_runtime_lock_uid=""
codexhub_runtime_lock_pid=""
codexhub_runtime_lock_starttime=""
codexhub_locked_runtime_floor=""
codexhub_locked_current_dir=""
codexhub_locked_current_entry=""
codexhub_locked_current_binary_relative_path=""
codexhub_locked_current_version=""

codexhub_runtime_process_identity() {
  identity_pid=$1
  codexhub_observed_uid=""
  codexhub_observed_starttime=""
  case "$identity_pid" in "" | *[!0-9]*) return 2 ;; esac
  identity_dir="$codexhub_runtime_lock_proc/$identity_pid"
  [ -d "$identity_dir" ] || return 1
  if [ ! -r "$identity_dir/status" ] || [ ! -r "$identity_dir/stat" ]; then
    [ -d "$identity_dir" ] && return 2
    return 1
  fi
  codexhub_observed_uid=$(awk '/^Uid:/ { print $2; exit }' "$identity_dir/status" 2>/dev/null)
  [ -n "$codexhub_observed_uid" ] || { [ -d "$identity_dir" ] && return 2; return 1; }
  identity_stat=$(sed -n '1p' "$identity_dir/stat" 2>/dev/null) || return 2
  identity_after_comm=${identity_stat##*) }
  [ "$identity_after_comm" != "$identity_stat" ] || return 2
  codexhub_observed_starttime=$(printf '%s\n' "$identity_after_comm" | awk '{ print $20 }')
  case "$codexhub_observed_starttime" in "" | *[!0-9]*) return 2 ;; esac
  return 0
}

codexhub_runtime_verify_lock_file() {
  lock_file=$1
  lock_kind=$2
  codexhub_verified_lock_uid=""
  codexhub_verified_lock_pid=""
  codexhub_verified_lock_starttime=""
  lock_name=${lock_file##*/}
  case "$lock_kind:$lock_name" in
    fixed:.codexhub-runtime-cleanup.lock) ;;
    candidate:.codexhub-runtime-cleanup.owner.*.*)
      lock_numbers=${lock_name#".codexhub-runtime-cleanup.owner."}
      lock_number_one=${lock_numbers%%.*}
      lock_number_two=${lock_numbers#"$lock_number_one."}
      case "$lock_number_one:$lock_number_two" in *[!0-9:]* | :* | *:) return 1 ;; esac
      ;;
    stale:.codexhub-runtime-cleanup.lock.stale.*.*)
      lock_numbers=${lock_name#".codexhub-runtime-cleanup.lock.stale."}
      lock_number_one=${lock_numbers%%.*}
      lock_number_two=${lock_numbers#"$lock_number_one."}
      case "$lock_number_one:$lock_number_two" in *[!0-9:]* | :* | *:) return 1 ;; esac
      ;;
    *) return 1 ;;
  esac
  [ -f "$lock_file" ] && [ ! -L "$lock_file" ] || return 1
  lock_parent_real=$(readlink -f "${lock_file%/*}" 2>/dev/null) || return 1
  lock_real=$(readlink -f "$lock_file" 2>/dev/null) || return 1
  [ "$lock_parent_real" = "$codexhub_runtime_lock_root_real" ] || return 1
  [ "$lock_real" = "$codexhub_runtime_lock_root_real/$lock_name" ] || return 1
  [ "$(wc -l <"$lock_file" 2>/dev/null | tr -d '[:space:]')" = 4 ] || return 1
  [ "$(sed -n '1p' "$lock_file" 2>/dev/null)" = "CodexHub runtime cleanup lock v1" ] || return 1
  lock_uid_line=$(sed -n '2p' "$lock_file" 2>/dev/null) || return 1
  lock_pid_line=$(sed -n '3p' "$lock_file" 2>/dev/null) || return 1
  lock_starttime_line=$(sed -n '4p' "$lock_file" 2>/dev/null) || return 1
  case "$lock_uid_line" in uid=*) lock_uid=${lock_uid_line#uid=} ;; *) return 1 ;; esac
  case "$lock_pid_line" in pid=*) lock_pid=${lock_pid_line#pid=} ;; *) return 1 ;; esac
  case "$lock_starttime_line" in starttime=*) lock_starttime=${lock_starttime_line#starttime=} ;; *) return 1 ;; esac
  case "$lock_uid:$lock_pid:$lock_starttime" in *[!0-9:]* | :* | *: | *::* ) return 1 ;; esac
  codexhub_verified_lock_uid=$lock_uid
  codexhub_verified_lock_pid=$lock_pid
  codexhub_verified_lock_starttime=$lock_starttime
  return 0
}

codexhub_runtime_lock_owner_activity() {
  owner_uid=$1
  owner_pid=$2
  owner_starttime=$3
  [ "$owner_uid" = "$codexhub_runtime_lock_uid" ] || return 2
  codexhub_runtime_process_identity "$owner_pid"
  identity_status=$?
  case "$identity_status" in
    0)
      if [ "$codexhub_observed_uid" = "$owner_uid" ] && [ "$codexhub_observed_starttime" = "$owner_starttime" ]; then
        return 0
      fi
      return 1
      ;;
    1) return 1 ;;
    *) return 2 ;;
  esac
}

codexhub_runtime_prepare_lock_candidate() {
  codexhub_runtime_lock_uid=$(id -u 2>/dev/null) || return 1
  codexhub_runtime_lock_pid=$$
  codexhub_runtime_process_identity "$codexhub_runtime_lock_pid" || return 1
  [ "$codexhub_observed_uid" = "$codexhub_runtime_lock_uid" ] || return 1
  codexhub_runtime_lock_starttime=$codexhub_observed_starttime
  codexhub_runtime_lock_candidate="$codexhub_runtime_lock_root/.codexhub-runtime-cleanup.owner.$codexhub_runtime_lock_pid.$codexhub_runtime_lock_starttime"
  [ ! -e "$codexhub_runtime_lock_candidate" ] && [ ! -L "$codexhub_runtime_lock_candidate" ] || return 1
  if ! {
    printf 'CodexHub runtime cleanup lock v1\n'
    printf 'uid=%s\n' "$codexhub_runtime_lock_uid"
    printf 'pid=%s\n' "$codexhub_runtime_lock_pid"
    printf 'starttime=%s\n' "$codexhub_runtime_lock_starttime"
  } >"$codexhub_runtime_lock_candidate" || ! chmod 600 "$codexhub_runtime_lock_candidate"; then
    rm -f "$codexhub_runtime_lock_candidate" 2>/dev/null || true
    return 1
  fi
  codexhub_runtime_verify_lock_file "$codexhub_runtime_lock_candidate" candidate || return 1
  [ "$codexhub_verified_lock_uid" = "$codexhub_runtime_lock_uid" ] || return 1
  [ "$codexhub_verified_lock_pid" = "$codexhub_runtime_lock_pid" ] || return 1
  [ "$codexhub_verified_lock_starttime" = "$codexhub_runtime_lock_starttime" ] || return 1
  return 0
}

codexhub_runtime_lock_assert_owned() {
  [ "$codexhub_runtime_lock_held" = yes ] || return 1
  codexhub_runtime_verify_lock_file "$codexhub_runtime_lock_path" fixed || return 1
  [ "$codexhub_verified_lock_uid" = "$codexhub_runtime_lock_uid" ] || return 1
  [ "$codexhub_verified_lock_pid" = "$codexhub_runtime_lock_pid" ] || return 1
  [ "$codexhub_verified_lock_starttime" = "$codexhub_runtime_lock_starttime" ] || return 1
  codexhub_runtime_lock_owner_activity "$codexhub_verified_lock_uid" "$codexhub_verified_lock_pid" "$codexhub_verified_lock_starttime"
  [ "$?" -eq 0 ]
}

codexhub_runtime_remove_stale_locks() {
  codexhub_runtime_lock_assert_owned || return 2
  for stale_lock in "$codexhub_runtime_lock_root"/.codexhub-runtime-cleanup.lock.stale.*; do
    if [ ! -e "$stale_lock" ] && [ ! -L "$stale_lock" ]; then continue; fi
    codexhub_runtime_verify_lock_file "$stale_lock" stale || return 2
    stale_uid=$codexhub_verified_lock_uid
    stale_pid=$codexhub_verified_lock_pid
    stale_starttime=$codexhub_verified_lock_starttime
    codexhub_runtime_lock_owner_activity "$stale_uid" "$stale_pid" "$stale_starttime"
    stale_activity=$?
    case "$stale_activity" in 0) return 3 ;; 1) ;; *) return 2 ;; esac
    codexhub_runtime_lock_assert_owned || return 2
    codexhub_runtime_verify_lock_file "$stale_lock" stale || return 2
    [ "$codexhub_verified_lock_uid" = "$stale_uid" ] || return 2
    [ "$codexhub_verified_lock_pid" = "$stale_pid" ] || return 2
    [ "$codexhub_verified_lock_starttime" = "$stale_starttime" ] || return 2
    codexhub_runtime_lock_owner_activity "$stale_uid" "$stale_pid" "$stale_starttime"
    [ "$?" -eq 1 ] || return 2
    rm -f "$stale_lock" || return 2
    [ ! -e "$stale_lock" ] && [ ! -L "$stale_lock" ] || return 2
  done
  return 0
}

codexhub_runtime_mv_no_replace_supported() {
  mv_probe_root="${TMPDIR:-/tmp}/codexhub-mv-no-replace-probe.$$"
  mv_probe_source="$mv_probe_root/source"
  mv_probe_destination="$mv_probe_root/destination"
  mv_probe_move_source="$mv_probe_root/move-source"
  mv_probe_move_destination="$mv_probe_root/move-destination"
  [ ! -e "$mv_probe_root" ] && [ ! -L "$mv_probe_root" ] || return 1
  mkdir "$mv_probe_root" 2>/dev/null || return 1
  if [ -e "$mv_probe_move_source" ] || [ -L "$mv_probe_move_source" ] ||
    [ -e "$mv_probe_move_destination" ] || [ -L "$mv_probe_move_destination" ]; then
    rmdir "$mv_probe_root" 2>/dev/null || true
    return 1
  fi
  printf 'source\n' >"$mv_probe_source" || { rmdir "$mv_probe_root" 2>/dev/null || true; return 1; }
  printf 'destination\n' >"$mv_probe_destination" || {
    rm -f "$mv_probe_source" 2>/dev/null || true
    rmdir "$mv_probe_root" 2>/dev/null || true
    return 1
  }
  printf 'move-source\n' >"$mv_probe_move_source" || {
    rm -f "$mv_probe_source" "$mv_probe_destination" 2>/dev/null || true
    rmdir "$mv_probe_root" 2>/dev/null || true
    return 1
  }
  mv -T -n "$mv_probe_source" "$mv_probe_destination" >/dev/null 2>&1
  mv_probe_status=$?
  mv -T -n "$mv_probe_move_source" "$mv_probe_move_destination" >/dev/null 2>&1
  mv_probe_move_status=$?
  mv_probe_collision_safe=no
  case "$mv_probe_status" in
    0 | 1) mv_probe_collision_safe=yes ;;
  esac
  mv_probe_safe=no
  # GNU coreutils 9.4 reports a protected no-clobber collision as status 1.
  # A second real move proves the flags work before either status is accepted.
  if [ "$mv_probe_collision_safe" = yes ] &&
    [ -f "$mv_probe_source" ] && [ ! -L "$mv_probe_source" ] &&
    [ -f "$mv_probe_destination" ] && [ ! -L "$mv_probe_destination" ] &&
    [ "$(wc -c <"$mv_probe_source" 2>/dev/null | tr -d '[:space:]')" = 7 ] &&
    [ "$(wc -c <"$mv_probe_destination" 2>/dev/null | tr -d '[:space:]')" = 12 ] &&
    [ "$(sed -n '1p' "$mv_probe_source" 2>/dev/null)" = source ] &&
    [ "$(sed -n '1p' "$mv_probe_destination" 2>/dev/null)" = destination ] &&
    [ "$mv_probe_move_status" -eq 0 ] &&
    [ ! -e "$mv_probe_move_source" ] && [ ! -L "$mv_probe_move_source" ] &&
    [ -f "$mv_probe_move_destination" ] && [ ! -L "$mv_probe_move_destination" ] &&
    [ "$(wc -c <"$mv_probe_move_destination" 2>/dev/null | tr -d '[:space:]')" = 12 ] &&
    [ "$(sed -n '1p' "$mv_probe_move_destination" 2>/dev/null)" = move-source ]; then
    mv_probe_safe=yes
  fi
  rm -f "$mv_probe_source" "$mv_probe_destination" \
    "$mv_probe_move_source" "$mv_probe_move_destination" 2>/dev/null || mv_probe_safe=no
  rmdir "$mv_probe_root" 2>/dev/null || mv_probe_safe=no
  [ "$mv_probe_safe" = yes ]
}

codexhub_runtime_lock_acquire() {
  for tool in readlink awk sed wc tr id rm mv mkdir chmod ln; do
    command -v "$tool" >/dev/null 2>&1 || return 4
  done
  codexhub_runtime_mv_no_replace_supported || return 4
  [ -d "$codexhub_runtime_lock_root" ] && [ ! -L "$codexhub_runtime_lock_root" ] || return 2
  codexhub_runtime_lock_root_real=$(readlink -f "$codexhub_runtime_lock_root" 2>/dev/null) || return 2
  [ "$codexhub_runtime_lock_root_real" = "$codexhub_runtime_lock_root" ] || return 2
  codexhub_runtime_prepare_lock_candidate || return 4
  if ln "$codexhub_runtime_lock_candidate" "$codexhub_runtime_lock_path" 2>/dev/null; then
    codexhub_runtime_lock_held=yes
    rm -f "$codexhub_runtime_lock_candidate" || return 4
    codexhub_runtime_lock_assert_owned || return 4
    codexhub_runtime_remove_stale_locks
    return $?
  fi
  rm -f "$codexhub_runtime_lock_candidate" 2>/dev/null || return 4
  codexhub_runtime_verify_lock_file "$codexhub_runtime_lock_path" fixed || return 2
  existing_uid=$codexhub_verified_lock_uid
  existing_pid=$codexhub_verified_lock_pid
  existing_starttime=$codexhub_verified_lock_starttime
  codexhub_runtime_lock_owner_activity "$existing_uid" "$existing_pid" "$existing_starttime"
  existing_activity=$?
  case "$existing_activity" in 0) return 3 ;; 1) ;; *) return 2 ;; esac
  stale_lock="$codexhub_runtime_lock_root/.codexhub-runtime-cleanup.lock.stale.$codexhub_runtime_lock_pid.$codexhub_runtime_lock_starttime"
  [ ! -e "$stale_lock" ] && [ ! -L "$stale_lock" ] || return 2
  codexhub_runtime_verify_lock_file "$codexhub_runtime_lock_path" fixed || return 2
  [ "$codexhub_verified_lock_uid" = "$existing_uid" ] || return 2
  [ "$codexhub_verified_lock_pid" = "$existing_pid" ] || return 2
  [ "$codexhub_verified_lock_starttime" = "$existing_starttime" ] || return 2
  codexhub_runtime_lock_owner_activity "$existing_uid" "$existing_pid" "$existing_starttime"
  [ "$?" -eq 1 ] || return 2
  mv -T -n "$codexhub_runtime_lock_path" "$stale_lock" 2>/dev/null || return 2
  [ ! -e "$codexhub_runtime_lock_path" ] && [ ! -L "$codexhub_runtime_lock_path" ] || return 2
  codexhub_runtime_verify_lock_file "$stale_lock" stale || return 2
  codexhub_runtime_prepare_lock_candidate || return 4
  if ! ln "$codexhub_runtime_lock_candidate" "$codexhub_runtime_lock_path" 2>/dev/null; then
    rm -f "$codexhub_runtime_lock_candidate" 2>/dev/null || true
    return 3
  fi
  codexhub_runtime_lock_held=yes
  rm -f "$codexhub_runtime_lock_candidate" || return 4
  codexhub_runtime_lock_assert_owned || return 4
  codexhub_runtime_remove_stale_locks
  return $?
}

codexhub_runtime_lock_release() {
  [ "$codexhub_runtime_lock_held" = yes ] || return 0
  codexhub_runtime_lock_assert_owned || return 1
  rm -f "$codexhub_runtime_lock_path" || return 1
  [ ! -e "$codexhub_runtime_lock_path" ] && [ ! -L "$codexhub_runtime_lock_path" ] || return 1
  codexhub_runtime_lock_held=no
  return 0
}

codexhub_runtime_normalized_version() {
  version_binary=$1
  [ -f "$version_binary" ] && [ -x "$version_binary" ] || return 1
  "$version_binary" --version 2>/dev/null | awk '
    NF { count += 1; value = $NF }
    END {
      sub(/^v/, "", value)
      if (count != 1 || value !~ /^[0-9A-Za-z.+-]+$/) exit 1
      split(value, build_parts, "+")
      split(build_parts[1], prerelease_parts, "-")
      number_count = split(prerelease_parts[1], numbers, ".")
      if (number_count < 2 || number_count > 4) exit 1
      for (number_index = 1; number_index <= number_count; number_index += 1) {
        if (numbers[number_index] !~ /^[0-9]+$/) exit 1
      }
      print value
    }
  '
}

codexhub_runtime_version_not_lower() {
  required_version=$1
  candidate_version=$2
  awk -v before="$required_version" -v after="$candidate_version" '
    function parse(value, numbers, metadata, prerelease, count, component_index) {
      sub(/^v/, "", value)
      split(value, metadata, "+")
      split(metadata[1], prerelease, "-")
      count = split(prerelease[1], numbers, ".")
      if (count < 2 || count > 4) return 0
      for (component_index = 1; component_index <= 4; component_index += 1) {
        if (component_index > count) numbers[component_index] = 0
        if (numbers[component_index] !~ /^[0-9]+$/) return 0
      }
      return 1
    }
    BEGIN {
      if (!parse(before, old_numbers, old_metadata, old_prerelease) ||
          !parse(after, new_numbers, new_metadata, new_prerelease)) exit 3
      for (component_index = 1; component_index <= 4; component_index += 1) {
        if ((new_numbers[component_index] + 0) > (old_numbers[component_index] + 0)) exit 0
        if ((new_numbers[component_index] + 0) < (old_numbers[component_index] + 0)) exit 2
      }
      old_has_prerelease = index(old_metadata[1], "-") > 0
      new_has_prerelease = index(new_metadata[1], "-") > 0
      if (old_has_prerelease && !new_has_prerelease) exit 0
      if (!old_has_prerelease && new_has_prerelease) exit 2
      if (old_metadata[1] == new_metadata[1]) exit 0
      exit 3
    }
  '
}

codexhub_runtime_consider_locked_floor() {
  observed_version=$1
  [ -n "$observed_version" ] || return 0
  if [ -z "$codexhub_locked_runtime_floor" ]; then
    codexhub_locked_runtime_floor=$observed_version
    return 0
  fi
  codexhub_runtime_version_not_lower "$codexhub_locked_runtime_floor" "$observed_version"
  observed_compare_status=$?
  case "$observed_compare_status" in
    0) codexhub_locked_runtime_floor=$observed_version ;;
    2) ;;
    *) return 1 ;;
  esac
  return 0
}

codexhub_runtime_capture_locked_state() {
  codexhub_locked_runtime_floor=""
  codexhub_locked_current_dir=""
  current_root="$HOME/.codex/packages/standalone"
  current_release_root="$current_root/releases"
  current_link="$current_root/current"
  if [ -e "$current_link" ] || [ -L "$current_link" ]; then
    [ -d "$current_release_root" ] && [ ! -L "$current_release_root" ] || return 1
    [ -L "$current_link" ] || return 1
    current_release_root_real=$(readlink -f "$current_release_root" 2>/dev/null) || return 1
    [ "$current_release_root_real" = "$current_release_root" ] || return 1
    current_dir_real=$(readlink -f "$current_link" 2>/dev/null) || return 1
    case "$current_dir_real" in "$current_release_root_real"/*) ;; *) return 1 ;; esac
    current_entry=${current_dir_real#"$current_release_root_real"/}
    case "$current_entry" in "" | "." | ".." | */* | *[!A-Za-z0-9._+-]*) return 1 ;; esac
    [ "$current_dir_real" = "$current_release_root_real/$current_entry" ] || return 1
    current_match_count=0
    for current_relative in bin/codex codex; do
      current_binary="$current_dir_real/$current_relative"
      if [ -e "$current_binary" ] || [ -L "$current_binary" ]; then
        current_match_count=$((current_match_count + 1))
        current_selected_binary=$current_binary
        current_selected_relative=$current_relative
      fi
    done
    [ "$current_match_count" -eq 1 ] || return 1
    [ -f "$current_selected_binary" ] && [ -x "$current_selected_binary" ] &&
      [ ! -L "$current_selected_binary" ] || return 1
    current_binary_real=$(readlink -f "$current_selected_binary" 2>/dev/null) || return 1
    [ "$current_binary_real" = "$current_dir_real/$current_selected_relative" ] || return 1
    current_version=$(codexhub_runtime_normalized_version "$current_selected_binary") || return 1
    case "$current_entry" in
      "$current_version") ;;
      "$current_version"-*) current_suffix=${current_entry#"$current_version"-}; case "$current_suffix" in "" | */* | *[!A-Za-z0-9._+-]*) return 1 ;; esac ;;
      *) return 1 ;;
    esac
    codexhub_locked_current_dir=$current_dir_real
    codexhub_locked_current_entry=$current_entry
    codexhub_locked_current_binary_relative_path=$current_selected_relative
    codexhub_locked_current_version=$current_version
    codexhub_runtime_consider_locked_floor "$current_version" || return 1
  fi
  active_path=$(command -v codex 2>/dev/null || true)
  if [ -n "$active_path" ]; then
    active_real=$(readlink -f "$active_path" 2>/dev/null) || return 1
    active_version=$(codexhub_runtime_normalized_version "$active_real") || return 1
    codexhub_runtime_consider_locked_floor "$active_version" || return 1
  fi
  login_shell=${SHELL:-}
  if [ -n "$login_shell" ] && [ -x "$login_shell" ] &&
    "$login_shell" -lc 'command -v codex >/dev/null 2>&1' 2>/dev/null; then
    login_version_raw=$("$login_shell" -lc 'codex --version' 2>/dev/null) || return 1
    login_version=$(printf '%s\n' "$login_version_raw" | awk '
      NF { count += 1; value = $NF }
      END {
        sub(/^v/, "", value)
        if (count != 1 || value !~ /^[0-9A-Za-z.+-]+$/) exit 1
        split(value, build_parts, "+")
        split(build_parts[1], prerelease_parts, "-")
        count = split(prerelease_parts[1], numbers, ".")
        if (count < 2 || count > 4) exit 1
        for (component_index = 1; component_index <= count; component_index += 1) if (numbers[component_index] !~ /^[0-9]+$/) exit 1
        print value
      }
    ') || return 1
    codexhub_runtime_consider_locked_floor "$login_version" || return 1
  fi
  return 0
}

codexhub_runtime_restore_locked_current() {
  [ -n "$codexhub_locked_current_dir" ] || return 0
  codexhub_runtime_lock_assert_owned || return 1
  locked_release_root="$HOME/.codex/packages/standalone/releases"
  locked_release_root_real=$(readlink -f "$locked_release_root" 2>/dev/null) || return 1
  locked_dir_real=$(readlink -f "$codexhub_locked_current_dir" 2>/dev/null) || return 1
  [ "$locked_release_root_real" = "$locked_release_root" ] || return 1
  [ "$locked_dir_real" = "$locked_release_root_real/$codexhub_locked_current_entry" ] || return 1
  locked_match_count=0
  for locked_relative in bin/codex codex; do
    locked_direct="$codexhub_locked_current_dir/$locked_relative"
    if [ -e "$locked_direct" ] || [ -L "$locked_direct" ]; then
      locked_match_count=$((locked_match_count + 1))
      locked_selected_relative=$locked_relative
    fi
  done
  [ "$locked_match_count" -eq 1 ] || return 1
  [ "$locked_selected_relative" = "$codexhub_locked_current_binary_relative_path" ] || return 1
  locked_binary="$codexhub_locked_current_dir/$codexhub_locked_current_binary_relative_path"
  [ -d "$codexhub_locked_current_dir" ] && [ ! -L "$codexhub_locked_current_dir" ] || return 1
  [ -f "$locked_binary" ] && [ -x "$locked_binary" ] && [ ! -L "$locked_binary" ] || return 1
  locked_binary_real=$(readlink -f "$locked_binary" 2>/dev/null) || return 1
  [ "$locked_binary_real" = "$locked_dir_real/$codexhub_locked_current_binary_relative_path" ] || return 1
  locked_version=$(codexhub_runtime_normalized_version "$locked_binary") || return 1
  [ "$locked_version" = "$codexhub_locked_current_version" ] || return 1
  current_link="$HOME/.codex/packages/standalone/current"
  if [ -e "$current_link" ] || [ -L "$current_link" ]; then [ -L "$current_link" ] || return 1; fi
  ln -sfn "$codexhub_locked_current_dir" "$current_link" || return 1
  restored_current=$(readlink -f "$current_link" 2>/dev/null) || return 1
  [ "$restored_current" = "$codexhub_locked_current_dir" ]
}

codexhub_runtime_verify_post_mutation_floor() {
  codexhub_runtime_lock_assert_owned || return 1
  saved_floor=$codexhub_locked_runtime_floor
  saved_current_dir=$codexhub_locked_current_dir
  saved_current_entry=$codexhub_locked_current_entry
  saved_current_relative=$codexhub_locked_current_binary_relative_path
  saved_current_version=$codexhub_locked_current_version
  codexhub_runtime_capture_locked_state
  post_capture_status=$?
  post_floor=$codexhub_locked_runtime_floor
  codexhub_locked_runtime_floor=$saved_floor
  codexhub_locked_current_dir=$saved_current_dir
  codexhub_locked_current_entry=$saved_current_entry
  codexhub_locked_current_binary_relative_path=$saved_current_relative
  codexhub_locked_current_version=$saved_current_version
  [ "$post_capture_status" -eq 0 ] || return 1
  [ -n "$post_floor" ] || return 1
  if [ -n "$saved_floor" ]; then
    codexhub_runtime_version_not_lower "$saved_floor" "$post_floor"
    [ "$?" -eq 0 ] || return 1
  fi
  return 0
}

trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'codexhub_runtime_lock_exit_status=$?; trap - EXIT; if [ "$codexhub_runtime_lock_exit_status" -ne 0 ]; then codexhub_runtime_restore_locked_current >/dev/null 2>&1 || codexhub_runtime_lock_exit_status=76; fi; codexhub_runtime_lock_release >/dev/null 2>&1 || true; exit "$codexhub_runtime_lock_exit_status"' EXIT
codexhub_runtime_lock_acquire
codexhub_runtime_lock_status=$?
case "$codexhub_runtime_lock_status" in
  0)
    if ! codexhub_runtime_capture_locked_state; then
      if [ "${codexhub_runtime_allow_unverified_state:-no}" = yes ]; then
        codexhub_locked_runtime_floor=""
        codexhub_locked_current_dir=""
      else
        printf 'The locked Codex runtime floor could not be verified safely.\n' >&2
        exit 75
      fi
    fi
    ;;
  3) printf 'Another verified CodexHub runtime operation is active.\n' >&2; exit 75 ;;
  2) printf 'The CodexHub runtime lock identity could not be verified safely.\n' >&2; exit 75 ;;
  *) printf 'The CodexHub runtime lock could not be acquired.\n' >&2; exit 75 ;;
esac
"###;

pub(crate) fn with_remote_codex_runtime_writer_lock(script: &str) -> String {
    format!("{REMOTE_CODEX_RUNTIME_WRITER_LOCK_PRELUDE}\n{script}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodexRuntimeReconcileStatus {
    Coordinated,
    Unchanged,
    NotInstalled,
    ManualRequired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodexRuntimeReconcileResult {
    pub(crate) status: CodexRuntimeReconcileStatus,
    pub(crate) target_changed: bool,
    pub(crate) launcher_changed: bool,
    pub(crate) target_version: Option<String>,
    pub(crate) launcher_version: Option<String>,
    pub(crate) login_shell_version: Option<String>,
    pub(crate) release_marked: bool,
    pub(crate) reason: String,
}

impl CodexRuntimeReconcileResult {
    pub(crate) fn completed(&self) -> bool {
        matches!(
            self.status,
            CodexRuntimeReconcileStatus::Coordinated
                | CodexRuntimeReconcileStatus::Unchanged
                | CodexRuntimeReconcileStatus::NotInstalled
        )
    }

    pub(crate) fn safe_summary(&self) -> String {
        let status = match self.status {
            CodexRuntimeReconcileStatus::Coordinated => "coordinated",
            CodexRuntimeReconcileStatus::Unchanged => "unchanged",
            CodexRuntimeReconcileStatus::NotInstalled => "not-installed",
            CodexRuntimeReconcileStatus::ManualRequired => "manual-required",
        };
        let version = self.target_version.as_deref().unwrap_or("unavailable");
        format!(
            "Remote Codex runtime [status={status}, targetChanged={}, launcherChanged={}, releaseMarked={}, version={version}, reason={}].",
            self.target_changed, self.launcher_changed, self.release_marked, self.reason
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodexReleaseCleanupStatus {
    Completed,
    Deferred,
    NotApplicable,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CodexReleaseCleanupPolicy {
    /// Install/Profile only remove releases already marked as CodexHub-managed.
    ManagedOnly,
    /// Update may adopt and move strictly older verified releases into staged backup.
    VerifiedOlderThan(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodexReleaseCleanupResult {
    pub(crate) status: CodexReleaseCleanupStatus,
    pub(crate) scanned_count: u32,
    pub(crate) adopted_count: u32,
    pub(crate) removed_count: u32,
    pub(crate) backed_up_count: u32,
    pub(crate) backup_id: Option<String>,
    pub(crate) ignored_session_process_count: u32,
    pub(crate) retained_count: u32,
    pub(crate) deferred_count: u32,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StrictCurrentRuntime {
    pub(crate) version: String,
    pub(crate) release_entry: String,
    pub(crate) binary_relative_path: String,
}

impl CodexReleaseCleanupResult {
    pub(crate) fn hard_failed(&self) -> bool {
        matches!(self.status, CodexReleaseCleanupStatus::Failed)
    }

    pub(crate) fn safe_summary(&self) -> String {
        let status = match self.status {
            CodexReleaseCleanupStatus::Completed => "completed",
            CodexReleaseCleanupStatus::Deferred => "deferred",
            CodexReleaseCleanupStatus::NotApplicable => "not-applicable",
            CodexReleaseCleanupStatus::Failed => "failed",
        };
        let backup_id = self.backup_id.as_deref().unwrap_or("none");
        format!(
            "Managed Codex runtime cleanup [status={status}, scanned={}, adopted={}, removed={}, backedUp={}, backupId={backup_id}, ignoredSessionProcesses={}, retained={}, deferred={}, reason={}].",
            self.scanned_count,
            self.adopted_count,
            self.removed_count,
            self.backed_up_count,
            self.ignored_session_process_count,
            self.retained_count,
            self.deferred_count,
            self.reason
        )
    }
}

fn marker_value<'a>(stdout: &'a str, marker: &str) -> Result<Option<&'a str>, String> {
    let prefix = format!("{marker}=");
    let mut matches = stdout.lines().filter_map(|line| line.strip_prefix(&prefix));
    let first = matches.next().map(str::trim);
    if matches.next().is_some() {
        return Err(format!(
            "Remote runtime returned duplicate {marker} markers."
        ));
    }
    Ok(first.filter(|value| !value.is_empty()))
}

fn parse_yes_no_marker(stdout: &str, marker: &str) -> Result<bool, String> {
    match marker_value(stdout, marker)? {
        Some("yes") => Ok(true),
        Some("no") => Ok(false),
        Some(_) => Err(format!(
            "Remote runtime returned an invalid {marker} marker."
        )),
        None => Err(format!("Remote runtime did not return {marker}.")),
    }
}

fn safe_marker_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn safe_version_marker(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
}

fn safe_release_entry(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-'))
}

fn release_entry_matches_version(entry: &str, version: &str) -> bool {
    entry == version
        || entry
            .strip_prefix(version)
            .and_then(|suffix| suffix.strip_prefix('-'))
            .is_some_and(|suffix| safe_release_entry(suffix))
}

fn parse_u32_marker(stdout: &str, marker: &str) -> Result<u32, String> {
    marker_value(stdout, marker)?
        .ok_or_else(|| format!("Remote cleanup did not return {marker}."))?
        .parse::<u32>()
        .map_err(|_| format!("Remote cleanup returned an invalid {marker} marker."))
}

/// Extracts the numeric Codex semver token used by remote runtime comparisons.
pub(crate) fn normalized_codex_version(value: &str) -> Option<String> {
    let token = value
        .split_whitespace()
        .last()
        .unwrap_or(value)
        .trim()
        .trim_start_matches('v');
    if token.is_empty()
        || token.len() > 64
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
    {
        return None;
    }
    let numeric = token
        .split_once('+')
        .map(|(value, _)| value)
        .unwrap_or(token)
        .split_once('-')
        .map(|(value, _)| value)
        .unwrap_or(token);
    let parts = numeric.split('.').collect::<Vec<_>>();
    if !(2..=4).contains(&parts.len())
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    Some(token.to_string())
}

pub(crate) fn remote_version_floor_prelude(
    minimum_version: Option<&str>,
    minimum_current_version: Option<&str>,
) -> Result<String, String> {
    let minimum = match minimum_version {
        Some(value) => normalized_codex_version(value).ok_or_else(|| {
            "The pre-operation Codex version could not be normalized safely.".to_string()
        })?,
        None => String::new(),
    };
    let minimum_current = match minimum_current_version {
        Some(value) => normalized_codex_version(value).ok_or_else(|| {
            "The pre-operation standalone/current version could not be normalized safely."
                .to_string()
        })?,
        None => String::new(),
    };
    Ok(format!(
        r###"codexhub_minimum_version='{minimum}'
codexhub_minimum_current_version='{minimum_current}'
codexhub_version_not_lower() {{
  before=$1
  after=$2
  awk -v before="$before" -v after="$after" '
    function parse(value, numbers, metadata, prerelease, count, component_index) {{
      sub(/^v/, "", value)
      split(value, metadata, "+")
      split(metadata[1], prerelease, "-")
      count = split(prerelease[1], numbers, ".")
      if (count < 2 || count > 4) return 0
      for (component_index = 1; component_index <= 4; component_index += 1) {{
        if (component_index > count) numbers[component_index] = 0
        if (numbers[component_index] !~ /^[0-9]+$/) return 0
      }}
      return 1
    }}
    BEGIN {{
      if (!parse(before, old_numbers, old_metadata, old_prerelease) ||
          !parse(after, new_numbers, new_metadata, new_prerelease)) exit 3
      for (component_index = 1; component_index <= 4; component_index += 1) {{
        if ((new_numbers[component_index] + 0) > (old_numbers[component_index] + 0)) exit 0
        if ((new_numbers[component_index] + 0) < (old_numbers[component_index] + 0)) exit 2
      }}
      old_has_prerelease = index(old_metadata[1], "-") > 0
      new_has_prerelease = index(new_metadata[1], "-") > 0
      if (old_has_prerelease && !new_has_prerelease) exit 0
      if (!old_has_prerelease && new_has_prerelease) exit 2
      if (old_metadata[1] == new_metadata[1]) exit 0
      exit 3
    }}
  '
}}
codexhub_version_meets_floors() {{
  candidate_version=$1
  for minimum_floor in "$codexhub_minimum_version" "$codexhub_minimum_current_version" "${{codexhub_locked_runtime_floor:-}}"; do
    [ -n "$minimum_floor" ] || continue
    codexhub_version_not_lower "$minimum_floor" "$candidate_version" || return 1
  done
  return 0
}}
"###
    ))
}

pub(crate) fn parse_remote_codex_release_cleanup_output(
    output: &ssh::SshCommandOutput,
) -> Result<CodexReleaseCleanupResult, String> {
    let status = match marker_value(&output.stdout, "CODEXHUB_CLEANUP_STATUS")? {
        Some("completed") => CodexReleaseCleanupStatus::Completed,
        Some("deferred") => CodexReleaseCleanupStatus::Deferred,
        Some("not-applicable") => CodexReleaseCleanupStatus::NotApplicable,
        Some("failed") => CodexReleaseCleanupStatus::Failed,
        Some(_) => return Err("Remote cleanup returned an unknown status marker.".into()),
        None => return Err("Remote cleanup did not return a status marker.".into()),
    };
    let reason = marker_value(&output.stdout, "CODEXHUB_CLEANUP_REASON")?
        .filter(|value| safe_marker_token(value))
        .ok_or_else(|| "Remote cleanup returned an unsafe or missing reason marker.".to_string())?;
    let backup_id = match marker_value(&output.stdout, "CODEXHUB_CLEANUP_BACKUP_ID")? {
        Some("none") => None,
        Some(value) if safe_marker_token(value) => Some(value.to_string()),
        Some(_) => return Err("Remote cleanup returned an unsafe backup identifier.".into()),
        None => return Err("Remote cleanup did not return a backup identifier.".into()),
    };
    let result = CodexReleaseCleanupResult {
        status,
        scanned_count: parse_u32_marker(&output.stdout, "CODEXHUB_CLEANUP_SCANNED")?,
        adopted_count: parse_u32_marker(&output.stdout, "CODEXHUB_CLEANUP_ADOPTED")?,
        removed_count: parse_u32_marker(&output.stdout, "CODEXHUB_CLEANUP_REMOVED")?,
        backed_up_count: parse_u32_marker(&output.stdout, "CODEXHUB_CLEANUP_BACKED_UP")?,
        backup_id,
        ignored_session_process_count: parse_u32_marker(
            &output.stdout,
            "CODEXHUB_CLEANUP_IGNORED_SESSION_PROCESSES",
        )?,
        retained_count: parse_u32_marker(&output.stdout, "CODEXHUB_CLEANUP_RETAINED")?,
        deferred_count: parse_u32_marker(&output.stdout, "CODEXHUB_CLEANUP_DEFERRED")?,
        reason: reason.to_string(),
    };
    let accounted_count = result
        .removed_count
        .checked_add(result.retained_count)
        .and_then(|count| count.checked_add(result.deferred_count))
        .ok_or_else(|| "Remote cleanup count markers overflowed.".to_string())?;
    if accounted_count != result.scanned_count {
        return Err("Remote cleanup returned inconsistent count markers.".into());
    }
    if result.adopted_count > result.scanned_count {
        return Err("Remote cleanup returned an impossible adopted count marker.".into());
    }
    if result.backed_up_count > result.removed_count
        || (result.backed_up_count > 0 && result.backup_id.is_none())
    {
        return Err("Remote cleanup returned inconsistent backup markers.".into());
    }
    match result.status {
        CodexReleaseCleanupStatus::Completed | CodexReleaseCleanupStatus::NotApplicable
            if !output.success() || result.deferred_count != 0 =>
        {
            return Err("Remote cleanup returned inconsistent completion markers.".into())
        }
        CodexReleaseCleanupStatus::Deferred if !output.success() || result.deferred_count == 0 => {
            return Err("Remote cleanup returned inconsistent deferred markers.".into())
        }
        CodexReleaseCleanupStatus::Failed if output.success() => {
            return Err("Remote cleanup reported failure with a successful command status.".into())
        }
        _ => {}
    }
    Ok(result)
}

pub(crate) fn parse_remote_codex_runtime_reconcile_output(
    output: &ssh::SshCommandOutput,
) -> Result<CodexRuntimeReconcileResult, String> {
    let status = match marker_value(&output.stdout, "CODEXHUB_RUNTIME_STATUS")? {
        Some("coordinated") => CodexRuntimeReconcileStatus::Coordinated,
        Some("unchanged") => CodexRuntimeReconcileStatus::Unchanged,
        Some("not-installed") => CodexRuntimeReconcileStatus::NotInstalled,
        Some("manual-required") => CodexRuntimeReconcileStatus::ManualRequired,
        Some(_) => return Err("Remote runtime returned an unknown status marker.".into()),
        None => return Err("Remote runtime did not return a status marker.".into()),
    };
    let reason = marker_value(&output.stdout, "CODEXHUB_RUNTIME_REASON")?
        .filter(|value| safe_marker_token(value))
        .ok_or_else(|| "Remote runtime returned an unsafe or missing reason marker.".to_string())?;
    let target_version =
        marker_value(&output.stdout, "CODEXHUB_RUNTIME_TARGET_VERSION")?.map(str::to_string);
    let launcher_version =
        marker_value(&output.stdout, "CODEXHUB_RUNTIME_LAUNCHER_VERSION")?.map(str::to_string);
    let login_shell_version =
        marker_value(&output.stdout, "CODEXHUB_RUNTIME_LOGIN_VERSION")?.map(str::to_string);
    if target_version
        .iter()
        .chain(launcher_version.iter())
        .chain(login_shell_version.iter())
        .any(|value| !safe_version_marker(value))
    {
        return Err("Remote runtime returned an unsafe version marker.".into());
    }
    let result = CodexRuntimeReconcileResult {
        status,
        target_changed: parse_yes_no_marker(&output.stdout, "CODEXHUB_RUNTIME_TARGET_CHANGED")?,
        launcher_changed: parse_yes_no_marker(&output.stdout, "CODEXHUB_RUNTIME_LAUNCHER_CHANGED")?,
        target_version,
        launcher_version,
        login_shell_version,
        release_marked: parse_yes_no_marker(&output.stdout, "CODEXHUB_RUNTIME_RELEASE_MARKED")?,
        reason: reason.to_string(),
    };
    if result.completed() && !output.success() {
        return Err("Remote runtime reported completion with a failed SSH command status.".into());
    }
    if matches!(
        result.status,
        CodexRuntimeReconcileStatus::Coordinated | CodexRuntimeReconcileStatus::Unchanged
    ) && (result.target_version.is_none()
        || result.launcher_version.is_none()
        || result.login_shell_version.is_none()
        || result.target_version != result.launcher_version
        || result.target_version != result.login_shell_version)
    {
        return Err("Remote runtime version markers were missing or inconsistent.".into());
    }
    match result.status {
        CodexRuntimeReconcileStatus::Coordinated
            if !result.target_changed && !result.launcher_changed =>
        {
            return Err(
                "Remote runtime reported coordinated without a managed-file change.".into(),
            );
        }
        CodexRuntimeReconcileStatus::Unchanged
            if result.target_changed || result.launcher_changed =>
        {
            return Err("Remote runtime reported unchanged with a managed-file change.".into());
        }
        CodexRuntimeReconcileStatus::NotInstalled => {
            if result.target_changed
                || result.launcher_changed
                || result.target_version.is_some()
                || result.launcher_version.is_some()
                || result.login_shell_version.is_some()
                || result.release_marked
            {
                return Err("Remote runtime returned inconsistent not-installed markers.".into());
            }
        }
        CodexRuntimeReconcileStatus::ManualRequired => {
            if output.success()
                || result.target_changed
                || result.launcher_changed
                || result.release_marked
            {
                return Err("Remote runtime returned inconsistent manual-repair markers.".into());
            }
        }
        _ => {}
    }
    Ok(result)
}

pub(crate) fn parse_remote_strict_current_version_output(
    output: &ssh::SshCommandOutput,
) -> Result<Option<StrictCurrentRuntime>, String> {
    let status = marker_value(&output.stdout, "CODEXHUB_CURRENT_STATUS")?.ok_or_else(|| {
        "Remote current-runtime probe did not return a status marker.".to_string()
    })?;
    let reason = marker_value(&output.stdout, "CODEXHUB_CURRENT_REASON")?
        .filter(|value| safe_marker_token(value))
        .ok_or_else(|| {
            "Remote current-runtime probe returned an unsafe or missing reason marker.".to_string()
        })?;
    let version = marker_value(&output.stdout, "CODEXHUB_CURRENT_VERSION")?;
    let release_entry = marker_value(&output.stdout, "CODEXHUB_CURRENT_RELEASE_ENTRY")?;
    let binary_relative_path = marker_value(&output.stdout, "CODEXHUB_CURRENT_BINARY_REL")?;
    match status {
        "absent"
            if output.success()
                && version.is_none()
                && release_entry.is_none()
                && binary_relative_path.is_none() =>
        {
            Ok(None)
        }
        "available" if output.success() => {
            let version = version.and_then(normalized_codex_version).ok_or_else(|| {
                "Remote current-runtime probe returned an invalid version.".to_string()
            })?;
            let release_entry = release_entry
                .filter(|entry| safe_release_entry(entry))
                .ok_or_else(|| {
                    "Remote current-runtime probe returned an invalid release entry.".to_string()
                })?;
            if !release_entry_matches_version(release_entry, &version) {
                return Err(
                    "Remote current-runtime release entry did not match its binary version.".into(),
                );
            }
            let binary_relative_path = match binary_relative_path {
                Some("bin/codex") => "bin/codex",
                Some("codex") => "codex",
                _ => {
                    return Err(
                        "Remote current-runtime probe returned an invalid binary layout.".into(),
                    )
                }
            };
            Ok(Some(StrictCurrentRuntime {
                version,
                release_entry: release_entry.to_string(),
                binary_relative_path: binary_relative_path.to_string(),
            }))
        }
        "failed"
            if !output.success()
                && version.is_none()
                && release_entry.is_none()
                && binary_relative_path.is_none() =>
        {
            Err(format!(
                "Remote standalone/current identity could not be verified ({reason})."
            ))
        }
        _ => Err("Remote current-runtime probe returned inconsistent markers.".into()),
    }
}

// Reads only a direct, canonical standalone/current release and never follows
// an unverified directory or binary identity into the update version floor.
pub(crate) const REMOTE_STRICT_CURRENT_VERSION_SCRIPT: &str = r###"set -u
release_root="$HOME/.codex/packages/standalone/releases"
current_link="$HOME/.codex/packages/standalone/current"

emit_current() {
  printf 'CODEXHUB_CURRENT_STATUS=%s\n' "$1"
  printf 'CODEXHUB_CURRENT_VERSION=%s\n' "$2"
  printf 'CODEXHUB_CURRENT_RELEASE_ENTRY=%s\n' "$3"
  printf 'CODEXHUB_CURRENT_BINARY_REL=%s\n' "$4"
  printf 'CODEXHUB_CURRENT_REASON=%s\n' "$5"
}
fail_current() {
  emit_current failed "" "" "" "$1"
  exit 1
}
if [ ! -e "$current_link" ] && [ ! -L "$current_link" ]; then
  emit_current absent "" "" "" no-standalone-current
  exit 0
fi
for tool in readlink awk; do
  command -v "$tool" >/dev/null 2>&1 || fail_current required-tool-unavailable
done
[ -d "$release_root" ] && [ ! -L "$release_root" ] || fail_current release-root-identity-unknown
[ -L "$current_link" ] || fail_current current-link-identity-unknown
release_root_real=$(readlink -f "$release_root" 2>/dev/null) || fail_current release-root-identity-unknown
current_real=$(readlink -f "$current_link" 2>/dev/null) || fail_current current-link-identity-unknown
[ "$release_root_real" = "$release_root" ] || fail_current release-root-parent-mismatch
case "$current_real" in
  "$release_root_real"/*) release_entry=${current_real#"$release_root_real"/} ;;
  *) fail_current current-link-outside-release-root ;;
esac
case "$release_entry" in "" | "." | ".." | */* | *[!A-Za-z0-9._+-]*) fail_current current-release-name-unsafe ;; esac
release_dir="$release_root/$release_entry"
[ -d "$release_dir" ] && [ ! -L "$release_dir" ] || fail_current current-release-identity-unknown
bin_layout=no
legacy_layout=no
{ [ -e "$release_dir/bin/codex" ] || [ -L "$release_dir/bin/codex" ]; } && bin_layout=yes
{ [ -e "$release_dir/codex" ] || [ -L "$release_dir/codex" ]; } && legacy_layout=yes
case "$bin_layout:$legacy_layout" in
  yes:no) binary_relative_path=bin/codex ;;
  no:yes) binary_relative_path=codex ;;
  yes:yes) fail_current current-binary-layout-ambiguous ;;
  *) fail_current current-binary-identity-unknown ;;
esac
binary="$release_dir/$binary_relative_path"
[ -f "$binary" ] && [ -x "$binary" ] && [ ! -L "$binary" ] ||
  fail_current current-binary-identity-unknown
release_real=$(readlink -f "$release_dir" 2>/dev/null) || fail_current current-release-identity-unknown
binary_real=$(readlink -f "$binary" 2>/dev/null) || fail_current current-binary-identity-unknown
[ "$release_real" = "$release_dir" ] || fail_current current-release-parent-mismatch
[ "$release_real" = "$current_real" ] || fail_current current-release-identity-changed
[ "$binary_real" = "$binary" ] || fail_current current-binary-parent-mismatch
reported_version=$("$binary" --version 2>/dev/null | awk '
  NF { count += 1; value = $NF }
  END {
    sub(/^v/, "", value)
    if (count != 1 || value !~ /^[0-9A-Za-z.+-]+$/) exit 1
    split(value, build_parts, "+")
    split(build_parts[1], prerelease_parts, "-")
    number_count = split(prerelease_parts[1], numbers, ".")
    if (number_count < 2 || number_count > 4) exit 1
    for (number_index = 1; number_index <= number_count; number_index += 1) if (numbers[number_index] !~ /^[0-9]+$/) exit 1
    print value
  }
') || fail_current current-version-unavailable
case "$release_entry" in
  "$reported_version") ;;
  "$reported_version"-*)
    vendor_suffix=${release_entry#"$reported_version"-}
    case "$vendor_suffix" in "" | "." | ".." | */* | *[!A-Za-z0-9._+-]*) fail_current current-version-mismatch ;; esac
    ;;
  *) fail_current current-version-mismatch ;;
esac
emit_current available "$reported_version" "$release_entry" "$binary_relative_path" current-runtime-verified
exit 0
"###;

// This script owns the single managed launcher template and coordinates its real target.
// It intentionally writes the literal standalone/current path instead of a resolved release path.
pub(crate) const REMOTE_CODEX_RUNTIME_RECONCILE_SCRIPT: &str = r###"set -u
umask 077
hub_dir="$HOME/.codex-hub"
launcher="$HOME/.local/bin/codex"
target_file="$hub_dir/codex-target"
standalone_root="$HOME/.codex/packages/standalone"
release_root="$standalone_root/releases"
current_link="$standalone_root/current"
standalone_current=""
release_marker_name=".codexhub-managed-release"
capture_marker_suffix=".codexhub-managed-capture"
target_changed=no
launcher_changed=no
release_marked=no
target_backup=""
launcher_backup=""
target_original_present=no
launcher_original_present=no
source_moved=no
capture_path=""
capture_name=""
capture_marker_path=""
capture_marker_created=no
candidate=""
legacy_release_dir=""
legacy_release_entry=""
legacy_release_version=""
target_version=""
launcher_version=""
login_version=""
exact_current_floor_available=no
floor_binary=""
floor_release_dir=""
floor_release_entry=""
floor_release_version=""
floor_binary_relative_path=""

mkdir -p "$hub_dir" "$HOME/.local/bin" || exit 1
chmod 700 "$hub_dir" || exit 1

emit_runtime_result() {
  printf 'CODEXHUB_RUNTIME_STATUS=%s\n' "$1"
  printf 'CODEXHUB_RUNTIME_TARGET_CHANGED=%s\n' "$target_changed"
  printf 'CODEXHUB_RUNTIME_LAUNCHER_CHANGED=%s\n' "$launcher_changed"
  printf 'CODEXHUB_RUNTIME_TARGET_VERSION=%s\n' "$target_version"
  printf 'CODEXHUB_RUNTIME_LAUNCHER_VERSION=%s\n' "$launcher_version"
  printf 'CODEXHUB_RUNTIME_LOGIN_VERSION=%s\n' "$login_version"
  printf 'CODEXHUB_RUNTIME_RELEASE_MARKED=%s\n' "$release_marked"
  printf 'CODEXHUB_RUNTIME_REASON=%s\n' "$2"
}

is_safe_component() {
  case "$1" in
    "" | "." | ".." | */* | *[!A-Za-z0-9._+-]*) return 1 ;;
    *) return 0 ;;
  esac
}

is_safe_target_text() {
  value=$1
  carriage_return=$(printf '\r')
  value_line_count=$(printf '%s\n' "$value" | wc -l | tr -d '[:space:]') || return 1
  [ "$value_line_count" = 1 ] || return 1
  case "$value" in
    /*) ;;
    *) return 1 ;;
  esac
  case "$value" in
    *"$carriage_return"*) return 1 ;;
  esac
  [ "$value" != "$launcher" ] || return 1
  [ "$value" != "$target_file" ] || return 1
  return 0
}

is_managed_launcher_path() {
  path=$1
  [ -f "$path" ] || return 1
  [ ! -L "$path" ] || return 1
  [ "$(sed -n '1p' "$path" 2>/dev/null)" = "#!/bin/sh" ] || return 1
  [ "$(sed -n '2p' "$path" 2>/dev/null)" = "# CodexHub managed launcher: loads remote API env before running real Codex." ]
}

is_valid_executable_target() {
  codexhub_target_validation_path=$1
  is_safe_target_text "$codexhub_target_validation_path" || return 1
  [ -f "$codexhub_target_validation_path" ] && [ -x "$codexhub_target_validation_path" ] || return 1
  is_managed_launcher_path "$codexhub_target_validation_path" && return 1
  command -v readlink >/dev/null 2>&1 || return 1
  target_real=$(readlink -f "$codexhub_target_validation_path" 2>/dev/null) || return 1
  is_safe_target_text "$target_real" || return 1
  [ -f "$target_real" ] && [ -x "$target_real" ] || return 1
  is_managed_launcher_path "$target_real" && return 1
  if [ -e "$launcher" ] || [ -L "$launcher" ]; then
    launcher_real=$(readlink -f "$launcher" 2>/dev/null) || return 1
    [ -n "$launcher_real" ] || return 1
    if [ "$target_real" = "$launcher_real" ]; then
      # Native installers may leave the public launcher as a symlink to the
      # verified current binary. Only that exact, independently rechecked path
      # is safe; arbitrary aliases could recurse after launcher replacement.
      [ -L "$launcher" ] || return 1
      codexhub_same_binary_target_path=$codexhub_target_validation_path
      codexhub_same_binary_target_real=$target_real
      select_verified_current_binary || return 1
      [ "$codexhub_same_binary_target_path" = "$standalone_current" ] || return 1
      codexhub_same_binary_current_real=$(readlink -f "$standalone_current" 2>/dev/null) || return 1
      [ "$codexhub_same_binary_current_real" = "$codexhub_same_binary_target_real" ] || return 1
      target_real=$codexhub_same_binary_target_real
    fi
  fi
  direct_target=$target_real
  if standalone_release_entry "$direct_target" >/dev/null 2>&1; then
    verify_direct_release_target "$direct_target" || return 1
  fi
  return 0
}

release_entry_matches_version() {
  release_entry=$1
  binary_version=$2
  case "$release_entry" in
    "$binary_version") return 0 ;;
    "$binary_version"-*)
      vendor_suffix=${release_entry#"$binary_version"-}
      is_safe_component "$vendor_suffix"
      return $?
      ;;
    *) return 1 ;;
  esac
}

standalone_release_entry() {
  path=$1
  case "$path" in
    "$release_root"/*/bin/codex | "$release_root"/*/codex)
      relative=${path#"$release_root"/}
      release_entry=${relative%%/*}
      binary_relative_path=${relative#"$release_entry"/}
      case "$binary_relative_path" in bin/codex | codex) ;; *) return 1 ;; esac
      is_safe_component "$release_entry" || return 1
      printf '%s\n' "$release_entry"
      return 0
      ;;
    *) return 1 ;;
  esac
}

normalized_version_for_path() {
  path=$1
  [ -f "$path" ] && [ -x "$path" ] || return 1
  raw_version=$("$path" --version 2>/dev/null) || return 1
  version_line=$(printf '%s\n' "$raw_version" | awk '
    NF { count += 1; value = $0 }
    END { if (count == 1) print value; else exit 1 }
  ') || return 1
  version_token=$(printf '%s\n' "$version_line" | awk '{ print $NF }')
  awk -v value="$version_token" '
    BEGIN {
      sub(/^v/, "", value)
      if (value !~ /^[0-9A-Za-z.+-]+$/) exit 1
      split(value, build_parts, "+")
      split(build_parts[1], prerelease_parts, "-")
      count = split(prerelease_parts[1], numbers, ".")
      if (count < 2 || count > 4) exit 1
      for (component_index = 1; component_index <= count; component_index += 1) {
        if (numbers[component_index] !~ /^[0-9]+$/) exit 1
      }
      print value
    }
  '
}

# Accepts only a literal canonical release entry with a supported binary layout.
verify_direct_release_target() {
  path=$1
  verified_release_dir=""
  verified_release_entry=""
  verified_release_version=""
  verified_binary_relative_path=""
  release_entry=$(standalone_release_entry "$path" 2>/dev/null) || return 1
  relative=${path#"$release_root/$release_entry"/}
  case "$relative" in bin/codex | codex) ;; *) return 1 ;; esac
  release_dir="$release_root/$release_entry"
  [ -d "$release_root" ] && [ ! -L "$release_root" ] || return 1
  [ -d "$release_dir" ] && [ ! -L "$release_dir" ] || return 1
  direct_layout_count=0
  selected_direct_relative=""
  for direct_relative in bin/codex codex; do
    direct_binary="$release_dir/$direct_relative"
    if [ -e "$direct_binary" ] || [ -L "$direct_binary" ]; then
      direct_layout_count=$((direct_layout_count + 1))
      selected_direct_relative=$direct_relative
    fi
  done
  [ "$direct_layout_count" -eq 1 ] || return 1
  [ "$selected_direct_relative" = "$relative" ] || return 1
  [ -f "$path" ] && [ -x "$path" ] && [ ! -L "$path" ] || return 1
  release_root_real=$(readlink -f "$release_root" 2>/dev/null) || return 1
  release_dir_real=$(readlink -f "$release_dir" 2>/dev/null) || return 1
  binary_real=$(readlink -f "$path" 2>/dev/null) || return 1
  [ "$release_root_real" = "$release_root" ] || return 1
  [ "$release_dir_real" = "$release_dir" ] || return 1
  [ "$release_dir_real" = "$release_root_real/$release_entry" ] || return 1
  [ "$binary_real" = "$path" ] || return 1
  [ "$binary_real" = "$release_dir_real/$relative" ] || return 1
  binary_version=$(normalized_version_for_path "$path" 2>/dev/null) || return 1
  release_entry_matches_version "$release_entry" "$binary_version" || return 1
  verified_release_dir=$release_dir
  verified_release_entry=$release_entry
  verified_release_version=$binary_version
  verified_binary_relative_path=$relative
  return 0
}

select_verified_current_binary() {
  standalone_current=""
  selected_current_entry=""
  selected_current_version=""
  selected_current_binary_relative_path=""
  [ -L "$current_link" ] || return 1
  current_dir_real=$(readlink -f "$current_link" 2>/dev/null) || return 1
  case "$current_dir_real" in "$release_root"/*) ;; *) return 1 ;; esac
  current_entry=${current_dir_real#"$release_root"/}
  is_safe_component "$current_entry" || return 1
  [ "$current_dir_real" = "$release_root/$current_entry" ] || return 1
  current_match_count=0
  for current_relative in bin/codex codex; do
    current_direct="$current_dir_real/$current_relative"
    if [ -e "$current_direct" ] || [ -L "$current_direct" ]; then
      current_match_count=$((current_match_count + 1))
      [ "$current_match_count" -eq 1 ] || return 1
      saved_current_direct=$current_direct
      saved_current_relative=$current_relative
    fi
  done
  [ "$current_match_count" -eq 1 ] || return 1
  verify_direct_release_target "$saved_current_direct" || return 1
  [ "$verified_release_entry" = "$current_entry" ] || return 1
  standalone_current="$current_link/$saved_current_relative"
  selected_current_entry=$verified_release_entry
  selected_current_version=$verified_release_version
  selected_current_binary_relative_path=$verified_binary_relative_path
  return 0
}

managed_release_marker_valid() {
  release_dir=$1
  release_version=$2
  marker="$release_dir/$release_marker_name"
  [ -f "$marker" ] && [ ! -L "$marker" ] || return 1
  [ "$(wc -l <"$marker" 2>/dev/null | tr -d '[:space:]')" = 2 ] || return 1
  [ "$(sed -n '1p' "$marker" 2>/dev/null)" = "CodexHub managed standalone release v1" ] || return 1
  [ "$(sed -n '2p' "$marker" 2>/dev/null)" = "version=$release_version" ] || return 1
}

mark_verified_release() {
  release_dir=$1
  release_version=$2
  codexhub_runtime_lock_assert_owned || return 1
  marker="$release_dir/$release_marker_name"
  if [ -e "$marker" ] || [ -L "$marker" ]; then
    managed_release_marker_valid "$release_dir" "$release_version" || return 1
    return 0
  fi
  marker_tmp="$release_dir/$release_marker_name.tmp.$$"
  [ ! -e "$marker_tmp" ] && [ ! -L "$marker_tmp" ] || return 1
  if ! {
    printf 'CodexHub managed standalone release v1\n'
    printf 'version=%s\n' "$release_version"
  } >"$marker_tmp" || ! chmod 600 "$marker_tmp" || ! mv "$marker_tmp" "$marker"; then
    rm -f "$marker_tmp"
    return 1
  fi
  release_marked=yes
  return 0
}

is_safe_capture_name() {
  value=$1
  case "$value" in codex-original.*.*) ;; *) return 1 ;; esac
  capture_numbers=${value#codex-original.}
  capture_nonce=${capture_numbers##*.}
  capture_timestamp=${capture_numbers%".$capture_nonce"}
  [ "$capture_numbers" = "$capture_timestamp.$capture_nonce" ] || return 1
  case "$capture_timestamp" in "" | *[!0-9]*) return 1 ;; esac
  case "$capture_nonce" in "" | *[!0-9]*) return 1 ;; esac
  return 0
}

verify_capture_binary() {
  path=$1
  expected_name=$2
  verified_capture_real=""
  verified_capture_version=""
  is_safe_capture_name "$expected_name" || return 1
  [ "$path" = "$hub_dir/$expected_name" ] || return 1
  [ -f "$path" ] && [ -x "$path" ] && [ ! -L "$path" ] || return 1
  hub_dir_real=$(readlink -f "$hub_dir" 2>/dev/null) || return 1
  capture_parent_real=$(readlink -f "${path%/*}" 2>/dev/null) || return 1
  capture_real=$(readlink -f "$path" 2>/dev/null) || return 1
  [ "$capture_parent_real" = "$hub_dir_real" ] || return 1
  [ "$capture_real" = "$hub_dir_real/$expected_name" ] || return 1
  capture_version=$(normalized_version_for_path "$path" 2>/dev/null) || return 1
  verified_capture_real=$capture_real
  verified_capture_version=$capture_version
  return 0
}

capture_marker_valid() {
  capture=$1
  expected_name=$2
  expected_version=$3
  marker="$capture$capture_marker_suffix"
  [ -f "$marker" ] && [ ! -L "$marker" ] || return 1
  [ "$(wc -l <"$marker" 2>/dev/null | tr -d '[:space:]')" = 3 ] || return 1
  [ "$(sed -n '1p' "$marker" 2>/dev/null)" = "CodexHub managed launcher capture v1" ] || return 1
  [ "$(sed -n '2p' "$marker" 2>/dev/null)" = "name=$expected_name" ] || return 1
  [ "$(sed -n '3p' "$marker" 2>/dev/null)" = "version=$expected_version" ] || return 1
}

verify_managed_capture() {
  capture=$1
  expected_name=$2
  verify_capture_binary "$capture" "$expected_name" || return 1
  capture_marker_valid "$capture" "$expected_name" "$verified_capture_version" || return 1
  return 0
}

write_capture_marker() {
  capture=$1
  expected_name=$2
  expected_version=$3
  codexhub_runtime_lock_assert_owned || return 1
  verify_capture_binary "$capture" "$expected_name" || return 1
  [ "$verified_capture_version" = "$expected_version" ] || return 1
  capture_marker_path="$capture$capture_marker_suffix"
  [ ! -e "$capture_marker_path" ] && [ ! -L "$capture_marker_path" ] || return 1
  capture_marker_tmp="$capture_marker_path.tmp.$$"
  [ ! -e "$capture_marker_tmp" ] && [ ! -L "$capture_marker_tmp" ] || return 1
  if ! {
    printf 'CodexHub managed launcher capture v1\n'
    printf 'name=%s\n' "$expected_name"
    printf 'version=%s\n' "$expected_version"
  } >"$capture_marker_tmp" || ! chmod 600 "$capture_marker_tmp" || ! mv "$capture_marker_tmp" "$capture_marker_path"; then
    rm -f "$capture_marker_tmp"
    return 1
  fi
  capture_marker_created=yes
  capture_marker_valid "$capture" "$expected_name" "$expected_version" || return 1
  return 0
}

version_not_lower() {
  before=$1
  after=$2
  awk -v before="$before" -v after="$after" '
    function parse(value, numbers, metadata, prerelease, count, component_index) {
      sub(/^v/, "", value)
      split(value, metadata, "+")
      split(metadata[1], prerelease, "-")
      count = split(prerelease[1], numbers, ".")
      if (count < 2 || count > 4) return 0
      for (component_index = 1; component_index <= 4; component_index += 1) {
        if (component_index > count) numbers[component_index] = 0
        if (numbers[component_index] !~ /^[0-9]+$/) return 0
      }
      return 1
    }
    BEGIN {
      if (!parse(before, old_numbers, old_metadata, old_prerelease) ||
          !parse(after, new_numbers, new_metadata, new_prerelease)) exit 3
      for (component_index = 1; component_index <= 4; component_index += 1) {
        if ((new_numbers[component_index] + 0) > (old_numbers[component_index] + 0)) exit 0
        if ((new_numbers[component_index] + 0) < (old_numbers[component_index] + 0)) exit 2
      }
      old_has_prerelease = index(old_metadata[1], "-") > 0
      new_has_prerelease = index(new_metadata[1], "-") > 0
      if (old_has_prerelease && !new_has_prerelease) exit 0
      if (!old_has_prerelease && new_has_prerelease) exit 2
      if (old_metadata[1] == new_metadata[1]) exit 0
      exit 3
    }
  '
}

login_shell_version() {
  login_shell=${SHELL:-/bin/sh}
  carriage_return=$(printf '\r')
  case "$login_shell" in
    /*) ;;
    *) return 1 ;;
  esac
  case "$login_shell" in
    *"$carriage_return"*) return 1 ;;
  esac
  [ -x "$login_shell" ] || return 1
  raw_version=$("$login_shell" -lc 'codex --version' 2>/dev/null) || return 1
  version_line=$(printf '%s\n' "$raw_version" | awk '
    NF { count += 1; value = $0 }
    END { if (count == 1) print value; else exit 1 }
  ') || return 1
  version_token=$(printf '%s\n' "$version_line" | awk '{ print $NF }')
  awk -v value="$version_token" '
    BEGIN {
      sub(/^v/, "", value)
      if (value !~ /^[0-9A-Za-z.+-]+$/) exit 1
      split(value, build_parts, "+")
      split(build_parts[1], prerelease_parts, "-")
      count = split(prerelease_parts[1], numbers, ".")
      if (count < 2 || count > 4) exit 1
      for (component_index = 1; component_index <= count; component_index += 1) {
        if (numbers[component_index] !~ /^[0-9]+$/) exit 1
      }
      print value
    }
  '
}

restore_target_file() {
  codexhub_runtime_lock_assert_owned || return 1
  [ "$target_changed" = yes ] || return 0
  if [ "$target_original_present" = yes ] && [ -n "$target_backup" ] && [ -f "$target_backup" ]; then
    restore_tmp="$target_file.codexhub.restore.$$"
    if cp -p "$target_backup" "$restore_tmp" 2>/dev/null; then
      chmod 600 "$restore_tmp" 2>/dev/null || true
      mv "$restore_tmp" "$target_file" 2>/dev/null || true
    fi
  else
    rm -f "$target_file"
  fi
}

restore_launcher() {
  codexhub_runtime_lock_assert_owned || return 1
  restore_failed=no
  if [ "$launcher_changed" = yes ]; then
    if [ "$source_moved" = yes ] && [ -n "$capture_path" ] && { [ -e "$capture_path" ] || [ -L "$capture_path" ]; }; then
      capture_restore_verified=yes
      if [ "$capture_marker_created" = yes ]; then
        verify_managed_capture "$capture_path" "$capture_name" || capture_restore_verified=no
      fi
      if [ "$capture_restore_verified" = yes ]; then
        rm -f "$launcher" 2>/dev/null || capture_restore_verified=no
        if [ -e "$launcher" ] || [ -L "$launcher" ]; then capture_restore_verified=no; fi
      fi
      if [ "$capture_restore_verified" = yes ] && mv "$capture_path" "$launcher" 2>/dev/null; then
        source_moved=no
        if [ "$capture_marker_created" = yes ] && [ -n "$capture_marker_path" ]; then
          rm -f "$capture_marker_path" 2>/dev/null || restore_failed=yes
          if [ -e "$capture_marker_path" ] || [ -L "$capture_marker_path" ]; then restore_failed=yes; fi
          capture_marker_created=no
        fi
      else
        restore_failed=yes
      fi
    elif [ "$launcher_original_present" = yes ] && [ -n "$launcher_backup" ]; then
      rm -f "$launcher" 2>/dev/null || restore_failed=yes
      if [ -L "$launcher_backup" ]; then
        cp -P "$launcher_backup" "$launcher" 2>/dev/null || restore_failed=yes
      elif [ -f "$launcher_backup" ]; then
        cp -p "$launcher_backup" "$launcher" 2>/dev/null || restore_failed=yes
      fi
    else
      rm -f "$launcher" 2>/dev/null || restore_failed=yes
    fi
  elif [ "$source_moved" = yes ]; then
    # A signal may arrive after rollback was armed but before mv ran. Infer the
    # completed state without overwriting an unexpectedly existing launcher.
    if { [ -e "$launcher" ] || [ -L "$launcher" ]; } &&
      [ ! -e "$capture_path" ] && [ ! -L "$capture_path" ]; then
      source_moved=no
    elif [ ! -e "$launcher" ] && [ ! -L "$launcher" ] &&
      [ -n "$capture_path" ] && { [ -e "$capture_path" ] || [ -L "$capture_path" ]; }; then
      capture_restore_verified=yes
      if [ "$capture_marker_created" = yes ]; then
        verify_managed_capture "$capture_path" "$capture_name" || capture_restore_verified=no
      fi
      if [ "$capture_restore_verified" = yes ] && mv "$capture_path" "$launcher" 2>/dev/null; then
        source_moved=no
        if [ "$capture_marker_created" = yes ] && [ -n "$capture_marker_path" ]; then
          rm -f "$capture_marker_path" 2>/dev/null || restore_failed=yes
          if [ -e "$capture_marker_path" ] || [ -L "$capture_marker_path" ]; then restore_failed=yes; fi
          capture_marker_created=no
        fi
      else
        restore_failed=yes
      fi
    else
      restore_failed=yes
    fi
  fi
  [ "$restore_failed" = no ]
}

fail_runtime() {
  reason=$1
  restore_launcher || reason=capture-rollback-failed
  restore_target_file
  target_changed=no
  launcher_changed=no
  target_version=""
  launcher_version=""
  login_version=""
  release_marked=no
  emit_runtime_result manual-required "$reason"
  exit 1
}

runtime_signal_failure() {
  trap - HUP INT TERM
  fail_runtime runtime-interrupted
}
trap runtime_signal_failure HUP INT TERM

# Re-selects the exact pre-operation standalone entry. This is deliberately
# independent from the installer-mutated current link and rejects dual layouts.
restore_exact_current_floor() {
  [ "$exact_current_floor_available" = yes ] || return 1
  verify_direct_release_target "$floor_binary" || return 1
  [ "$verified_release_dir" = "$floor_release_dir" ] || return 1
  [ "$verified_release_entry" = "$floor_release_entry" ] || return 1
  [ "$verified_release_version" = "$floor_release_version" ] || return 1
  [ "$verified_binary_relative_path" = "$floor_binary_relative_path" ] || return 1
  if [ -e "$current_link" ] || [ -L "$current_link" ]; then
    [ -L "$current_link" ] || return 1
  fi
  codexhub_runtime_lock_assert_owned || return 1
  ln -sfn "$floor_release_dir" "$current_link" || return 1
  select_verified_current_binary || return 1
  [ "$selected_current_entry" = "$floor_release_entry" ] || return 1
  [ "$selected_current_version" = "$floor_release_version" ] || return 1
  [ "$selected_current_binary_relative_path" = "$floor_binary_relative_path" ] || return 1
  return 0
}

prefer_exact_current_candidate() {
  restore_exact_current_floor || return 1
  candidate=$standalone_current
  target_version=$(normalized_version_for_path "$candidate" 2>/dev/null) || return 1
  [ "$target_version" = "$floor_release_version" ] || return 1
  return 0
}

enforce_candidate_floor() {
  required_floor=$1
  below_reason=$2
  incomparable_reason=$3
  version_not_lower "$required_floor" "$target_version"
  floor_compare_status=$?
  if [ "$floor_compare_status" -ne 0 ] && select_verified_current_binary; then
    current_recovery_version=$(normalized_version_for_path "$standalone_current" 2>/dev/null || true)
    if [ -n "$current_recovery_version" ]; then
      version_not_lower "$required_floor" "$current_recovery_version"
      if [ "$?" -eq 0 ]; then
        candidate=$standalone_current
        target_version=$current_recovery_version
        floor_compare_status=0
      fi
    fi
  fi
  if [ "$floor_compare_status" -ne 0 ] && [ "$exact_current_floor_available" = yes ]; then
    prefer_exact_current_candidate || fail_runtime exact-current-restore-failed
    version_not_lower "$required_floor" "$target_version"
    floor_compare_status=$?
  fi
  case "$floor_compare_status" in
    0) ;;
    2) fail_runtime "$below_reason" ;;
    *) fail_runtime "$incomparable_reason" ;;
  esac
}

# Validate the exact current floor before trusting the installer-mutated current
# selector. It remains available even if current was deleted or made ambiguous.
minimum_current_version=${codexhub_minimum_current_version:-}
minimum_current_entry=${codexhub_minimum_current_entry:-}
minimum_current_binary_relative_path=${codexhub_minimum_current_binary_relative_path:-}
if [ -n "$minimum_current_version" ]; then
  normalized_minimum_current=$(printf '%s\n' "$minimum_current_version" | awk '
    BEGIN { value = "" }
    NF { count += 1; value = $NF }
    END {
      sub(/^v/, "", value)
      if (count != 1 || value !~ /^[0-9A-Za-z.+-]+$/) exit 1
      split(value, build_parts, "+")
      split(build_parts[1], prerelease_parts, "-")
      number_count = split(prerelease_parts[1], numbers, ".")
      if (number_count < 2 || number_count > 4) exit 1
      for (number_index = 1; number_index <= number_count; number_index += 1) {
        if (numbers[number_index] !~ /^[0-9]+$/) exit 1
      }
      print value
    }
  ') || fail_runtime minimum-current-version-invalid
  is_safe_component "$minimum_current_entry" || fail_runtime minimum-current-entry-invalid
  release_entry_matches_version "$minimum_current_entry" "$normalized_minimum_current" || fail_runtime minimum-current-entry-mismatch
  case "$minimum_current_binary_relative_path" in bin/codex | codex) ;; *) fail_runtime minimum-current-layout-invalid ;; esac
  floor_binary="$release_root/$minimum_current_entry/$minimum_current_binary_relative_path"
  verify_direct_release_target "$floor_binary" || fail_runtime minimum-current-release-unavailable
  floor_release_dir=$verified_release_dir
  floor_release_entry=$verified_release_entry
  floor_release_version=$verified_release_version
  floor_binary_relative_path=$verified_binary_relative_path
  [ "$floor_release_version" = "$normalized_minimum_current" ] || fail_runtime minimum-current-release-mismatch
  [ "$floor_release_entry" = "$minimum_current_entry" ] || fail_runtime minimum-current-entry-changed
  [ "$floor_binary_relative_path" = "$minimum_current_binary_relative_path" ] || fail_runtime minimum-current-layout-changed
  exact_current_floor_available=yes
elif [ -n "$minimum_current_entry" ] || [ -n "$minimum_current_binary_relative_path" ]; then
  fail_runtime minimum-current-markers-inconsistent
fi

# A present standalone/current must have exactly one direct executable layout.
# When the exact floor is known, repair a missing, ambiguous, or regressed link
# before any ordinary candidate floor can reject the recovery path.
current_verified=no
if [ -e "$current_link" ] || [ -L "$current_link" ]; then
  if select_verified_current_binary; then
    current_verified=yes
  elif [ "$exact_current_floor_available" = yes ] && restore_exact_current_floor; then
    current_verified=yes
  else
    fail_runtime current-release-identity-unknown
  fi
elif [ "$exact_current_floor_available" = yes ]; then
  restore_exact_current_floor || fail_runtime exact-current-restore-failed
  current_verified=yes
fi
if [ "$current_verified" = yes ] && [ "$exact_current_floor_available" = yes ]; then
  version_not_lower "$normalized_minimum_current" "$selected_current_version"
  current_floor_status=$?
  if [ "$current_floor_status" -ne 0 ]; then
    restore_exact_current_floor || fail_runtime exact-current-restore-failed
  fi
fi

# Capture the login-shell-visible runtime only after exact current recovery, but
# still before any managed launcher or target write.
pre_login_version=""
pre_login_shell=${SHELL:-/bin/sh}
carriage_return=$(printf '\r')
case "$pre_login_shell" in /*) ;; *) fail_runtime pre-login-shell-invalid ;; esac
case "$pre_login_shell" in *"$carriage_return"*) fail_runtime pre-login-shell-invalid ;; esac
[ -x "$pre_login_shell" ] || fail_runtime pre-login-shell-invalid
if "$pre_login_shell" -lc 'command -v codex >/dev/null 2>&1' 2>/dev/null; then
  pre_login_version=$(login_shell_version 2>/dev/null) || fail_runtime pre-login-version-unknown
fi

target_file_value=""
target_file_present=no
normalized_target_file_value=""
target_capture_managed=no
target_capture_version=""
if [ -e "$target_file" ] || [ -L "$target_file" ]; then
  target_original_present=yes
  if [ -L "$target_file" ] || [ ! -f "$target_file" ]; then
    fail_runtime target-file-identity-unknown
  fi
  target_line_count=$(wc -l <"$target_file" 2>/dev/null | tr -d '[:space:]') || fail_runtime target-file-unreadable
  [ "$target_line_count" = 1 ] || fail_runtime target-file-invalid-lines
  target_file_value=$(sed -n '1p' "$target_file" 2>/dev/null) || fail_runtime target-file-unreadable
  target_file_bytes=$(wc -c <"$target_file" 2>/dev/null | tr -d '[:space:]') || fail_runtime target-file-unreadable
  target_value_bytes=$(printf '%s\n' "$target_file_value" | wc -c | tr -d '[:space:]') || fail_runtime target-file-unreadable
  [ "$target_file_bytes" = "$target_value_bytes" ] || fail_runtime target-file-invalid-lines
  is_safe_target_text "$target_file_value" || fail_runtime target-file-invalid-path
  if standalone_release_entry "$target_file_value" >/dev/null 2>&1; then
    verify_direct_release_target "$target_file_value" || fail_runtime target-release-identity-unknown
    legacy_release_dir=$verified_release_dir
    legacy_release_entry=$verified_release_entry
    legacy_release_version=$verified_release_version
    legacy_binary_relative_path=$verified_binary_relative_path
    # A missing current link must not hide a separately verified legacy target.
    # Existing invalid current state was already rejected above.
    if select_verified_current_binary; then
      normalized_target_file_value="$standalone_current"
    else
      normalized_target_file_value="$target_file_value"
    fi
  else
    is_valid_executable_target "$target_file_value" || fail_runtime target-file-identity-unknown
    normalized_target_file_value="$target_file_value"
    target_capture_name=${target_file_value##*/}
    if verify_managed_capture "$target_file_value" "$target_capture_name"; then
      target_capture_managed=yes
      target_capture_version=$verified_capture_version
    fi
  fi
  target_file_present=yes
fi

launcher_kind=absent
before_version=""
capture_needed=no
launcher_symlink_real=""
if [ -e "$launcher" ] || [ -L "$launcher" ]; then
  launcher_original_present=yes
  if is_managed_launcher_path "$launcher"; then
    launcher_kind=managed
    [ -x "$launcher" ] || fail_runtime existing-managed-launcher-invalid
    before_version=$(normalized_version_for_path "$launcher" 2>/dev/null) || fail_runtime existing-managed-launcher-version-unknown
  else
    launcher_kind=external
    [ -f "$launcher" ] && [ -x "$launcher" ] || fail_runtime existing-launcher-invalid
    before_version=$(normalized_version_for_path "$launcher" 2>/dev/null) || fail_runtime existing-launcher-version-unknown
    if [ -L "$launcher" ]; then
      command -v readlink >/dev/null 2>&1 || fail_runtime existing-launcher-identity-unknown
      launcher_symlink_real=$(readlink -f "$launcher" 2>/dev/null) || fail_runtime existing-launcher-identity-unknown
      is_safe_target_text "$launcher_symlink_real" || fail_runtime existing-launcher-identity-unknown
      [ -f "$launcher_symlink_real" ] && [ -x "$launcher_symlink_real" ] || fail_runtime existing-launcher-invalid
      is_managed_launcher_path "$launcher_symlink_real" && fail_runtime existing-launcher-self-reference
    fi
  fi
fi

# A previously managed launcher proves CodexHub ownership of a strictly
# verified legacy target. Mark it before migration so cleanup can reclaim it.
if [ "$launcher_kind" = managed ] && [ -n "$legacy_release_dir" ]; then
  if ! verify_direct_release_target "$target_file_value" ||
    [ "$verified_release_dir" != "$legacy_release_dir" ] ||
    [ "$verified_release_entry" != "$legacy_release_entry" ] ||
    [ "$verified_binary_relative_path" != "$legacy_binary_relative_path" ] ||
    [ "$verified_release_version" != "$legacy_release_version" ]; then
    fail_runtime legacy-release-identity-changed
  fi
  mark_verified_release "$legacy_release_dir" "$legacy_release_version" || fail_runtime legacy-release-marker-invalid
fi

if [ "$launcher_kind" = external ]; then
  if [ -L "$launcher" ]; then
    if standalone_release_entry "$launcher_symlink_real" >/dev/null 2>&1; then
      verify_direct_release_target "$launcher_symlink_real" || fail_runtime launcher-release-identity-unknown
      select_verified_current_binary || fail_runtime current-release-identity-unknown
      candidate="$standalone_current"
    else
      candidate="$launcher_symlink_real"
    fi
  else
    timestamp=$(date +%Y%m%d%H%M%S 2>/dev/null) || fail_runtime capture-name-unavailable
    case "$timestamp" in "" | *[!0-9]*) fail_runtime capture-name-invalid ;; esac
    case "$$" in "" | *[!0-9]*) fail_runtime capture-name-invalid ;; esac
    capture_name="codex-original.$timestamp.$$"
    is_safe_capture_name "$capture_name" || fail_runtime capture-name-invalid
    capture_path="$hub_dir/$capture_name"
    [ ! -e "$capture_path" ] && [ ! -L "$capture_path" ] || fail_runtime capture-path-collision
    candidate="$capture_path"
    capture_needed=yes
  fi
elif [ "$target_file_present" = yes ]; then
  candidate="$normalized_target_file_value"
  if [ "$target_capture_managed" = yes ] && select_verified_current_binary; then
    current_candidate_version=$(normalized_version_for_path "$standalone_current" 2>/dev/null || true)
    if [ -n "$current_candidate_version" ]; then
      version_not_lower "$target_capture_version" "$current_candidate_version"
      current_candidate_status=$?
      if [ "$current_candidate_status" -eq 0 ]; then
        candidate=$standalone_current
      fi
    fi
  fi
elif select_verified_current_binary; then
  candidate="$standalone_current"
else
  discovered=$(command -v codex 2>/dev/null || true)
  if [ -z "$discovered" ]; then
    emit_runtime_result not-installed no-codex-entry
    exit 0
  fi
  is_safe_target_text "$discovered" || fail_runtime discovered-entry-invalid
  candidate="$discovered"
  before_version=$(normalized_version_for_path "$candidate" 2>/dev/null) || fail_runtime discovered-entry-version-unknown
fi

is_safe_target_text "$candidate" || fail_runtime selected-target-invalid

if [ "$capture_needed" = yes ]; then
  codexhub_runtime_lock_assert_owned || fail_runtime runtime-lock-lost
  # Arm rollback before mv because POSIX shells dispatch a pending signal as
  # soon as the foreground command returns.
  source_moved=yes
  if ! mv "$launcher" "$capture_path"; then
    fail_runtime existing-launcher-capture-failed
  fi
  verify_capture_binary "$capture_path" "$capture_name" || fail_runtime captured-launcher-invalid
  captured_version=$verified_capture_version
  [ "$captured_version" = "$before_version" ] || fail_runtime captured-launcher-version-changed
  write_capture_marker "$capture_path" "$capture_name" "$captured_version" || fail_runtime capture-marker-write-failed
  verify_managed_capture "$capture_path" "$capture_name" || fail_runtime captured-launcher-unconfirmed
  if select_verified_current_binary; then
    current_candidate_version=$(normalized_version_for_path "$standalone_current" 2>/dev/null || true)
    if [ -n "$current_candidate_version" ]; then
      version_not_lower "$captured_version" "$current_candidate_version"
      current_candidate_status=$?
      if [ "$current_candidate_status" -eq 0 ]; then
        candidate=$standalone_current
      fi
    fi
  fi
else
  is_valid_executable_target "$candidate" || fail_runtime selected-target-identity-unknown
fi

[ -f "$candidate" ] && [ -x "$candidate" ] || fail_runtime selected-target-not-executable
is_managed_launcher_path "$candidate" && fail_runtime selected-target-is-managed-launcher

target_version=$(normalized_version_for_path "$candidate" 2>/dev/null) || fail_runtime selected-target-version-unknown
if [ -n "${codexhub_locked_runtime_floor:-}" ]; then
  enforce_candidate_floor "$codexhub_locked_runtime_floor" selected-target-below-locked-runtime locked-runtime-version-incomparable
fi
if [ -n "$before_version" ]; then
  enforce_candidate_floor "$before_version" selected-target-would-downgrade selected-target-version-incomparable
fi
if [ -n "$pre_login_version" ]; then
  enforce_candidate_floor "$pre_login_version" selected-target-below-pre-login-runtime pre-login-version-incomparable
fi

minimum_version=${codexhub_minimum_version:-}
if [ -n "$minimum_version" ]; then
  normalized_minimum=$(printf '%s\n' "$minimum_version" | awk '
    BEGIN { value = "" }
    NF { count += 1; value = $NF }
    END {
      sub(/^v/, "", value)
      if (count != 1 || value !~ /^[0-9A-Za-z.+-]+$/) exit 1
      split(value, build_parts, "+")
      split(build_parts[1], prerelease_parts, "-")
      number_count = split(prerelease_parts[1], numbers, ".")
      if (number_count < 2 || number_count > 4) exit 1
      for (number_index = 1; number_index <= number_count; number_index += 1) {
        if (numbers[number_index] !~ /^[0-9]+$/) exit 1
      }
      print value
    }
  ') || fail_runtime minimum-version-invalid
  enforce_candidate_floor "$normalized_minimum" runtime-version-below-operation-start minimum-version-incomparable
fi

if [ "$exact_current_floor_available" = yes ]; then
  enforce_candidate_floor "$normalized_minimum_current" runtime-version-below-current-floor minimum-current-version-incomparable
fi

codexhub_runtime_lock_assert_owned || fail_runtime runtime-lock-lost
target_tmp="$target_file.codexhub.tmp.$$"
if ! printf '%s\n' "$candidate" >"$target_tmp" || ! chmod 600 "$target_tmp"; then
  rm -f "$target_tmp"
  fail_runtime target-stage-failed
fi
if [ -f "$target_file" ] && cmp -s "$target_tmp" "$target_file"; then
  rm -f "$target_tmp"
else
  if [ "$target_original_present" = yes ]; then
    timestamp=$(date +%Y%m%d%H%M%S 2>/dev/null || printf 'unknown')
    target_backup="$target_file.codexhub.bak.$timestamp.$$"
    if ! cp -p "$target_file" "$target_backup"; then
      rm -f "$target_tmp"
      fail_runtime target-backup-failed
    fi
  fi
  # Rollback state must precede the atomic replace for signal safety.
  target_changed=yes
  if ! mv "$target_tmp" "$target_file"; then
    rm -f "$target_tmp"
    fail_runtime target-replace-failed
  fi
fi

codexhub_runtime_lock_assert_owned || fail_runtime runtime-lock-lost
launcher_tmp="$launcher.codexhub.tmp.$$"
cat >"$launcher_tmp" <<'CODEXHUB_CODEX_LAUNCHER'
#!/bin/sh
# CodexHub managed launcher: loads remote API env before running real Codex.
set -u
env_file="$HOME/.codex-hub/env"
target_file="$HOME/.codex-hub/codex-target"
launcher="$HOME/.local/bin/codex"
if [ -f "$env_file" ] && [ ! -L "$env_file" ]; then
  . "$env_file"
fi
if [ ! -f "$target_file" ] || [ -L "$target_file" ]; then
  printf 'CodexHub launcher target file is missing or unsafe.\n' >&2
  exit 127
fi
target_line_count=$(wc -l <"$target_file" 2>/dev/null | tr -d '[:space:]')
if [ "$target_line_count" != 1 ]; then
  printf 'CodexHub launcher target file is invalid.\n' >&2
  exit 127
fi
target=$(sed -n '1p' "$target_file" 2>/dev/null)
target_file_bytes=$(wc -c <"$target_file" 2>/dev/null | tr -d '[:space:]')
target_value_bytes=$(printf '%s\n' "$target" | wc -c | tr -d '[:space:]')
if [ -z "$target_file_bytes" ] || [ "$target_file_bytes" != "$target_value_bytes" ]; then
  printf 'CodexHub launcher target file is invalid.\n' >&2
  exit 127
fi
carriage_return=$(printf '\r')
case "$target" in
  /*) ;;
  *) printf 'CodexHub launcher target is invalid.\n' >&2; exit 127 ;;
esac
case "$target" in
  *"$carriage_return"*) printf 'CodexHub launcher target is invalid.\n' >&2; exit 127 ;;
esac
if [ "$target" = "$launcher" ] || [ ! -f "$target" ] || [ ! -x "$target" ]; then
  printf 'CodexHub launcher target is not executable.\n' >&2
  exit 127
fi
if ! command -v readlink >/dev/null 2>&1; then
  printf 'CodexHub launcher target identity cannot be verified.\n' >&2
  exit 127
fi
target_real=$(readlink -f "$target" 2>/dev/null || true)
launcher_real=$(readlink -f "$launcher" 2>/dev/null || true)
if [ -z "$target_real" ] || [ -z "$launcher_real" ] || [ "$target_real" = "$launcher_real" ]; then
  printf 'CodexHub launcher target identity is unsafe.\n' >&2
  exit 127
fi
exec "$target" "$@"
CODEXHUB_CODEX_LAUNCHER
launcher_stage_status=$?
if [ "$launcher_stage_status" -ne 0 ] || ! chmod 700 "$launcher_tmp"; then
  rm -f "$launcher_tmp"
  fail_runtime launcher-stage-failed
fi

if [ -f "$launcher" ] && [ ! -L "$launcher" ] && cmp -s "$launcher_tmp" "$launcher"; then
  rm -f "$launcher_tmp"
else
  if [ "$launcher_original_present" = yes ] && [ "$source_moved" = no ]; then
    timestamp=$(date +%Y%m%d%H%M%S 2>/dev/null || printf 'unknown')
    launcher_backup="$launcher.codexhub.bak.$timestamp.$$"
    if [ -L "$launcher" ]; then
      if ! cp -P "$launcher" "$launcher_backup"; then
        rm -f "$launcher_tmp"
        fail_runtime launcher-backup-failed
      fi
    elif ! cp -p "$launcher" "$launcher_backup"; then
      rm -f "$launcher_tmp"
      fail_runtime launcher-backup-failed
    fi
  elif [ "$source_moved" = yes ]; then
    launcher_backup="$capture_path"
  fi
  # Rollback state must precede the atomic replace for signal safety.
  launcher_changed=yes
  if ! mv "$launcher_tmp" "$launcher"; then
    rm -f "$launcher_tmp"
    fail_runtime launcher-replace-failed
  fi
fi

launcher_version=$(normalized_version_for_path "$launcher" 2>/dev/null) || fail_runtime launcher-validation-failed
login_version=$(login_shell_version 2>/dev/null) || fail_runtime login-shell-validation-failed
target_version=$(normalized_version_for_path "$candidate" 2>/dev/null) || fail_runtime target-validation-failed
if [ "$target_version" != "$launcher_version" ] || [ "$target_version" != "$login_version" ]; then
  fail_runtime runtime-version-mismatch
fi
if [ -n "$before_version" ]; then
  version_not_lower "$before_version" "$target_version"
  version_compare_status=$?
  [ "$version_compare_status" -eq 0 ] || fail_runtime runtime-version-regressed
fi
if [ -n "$pre_login_version" ]; then
  version_not_lower "$pre_login_version" "$target_version"
  pre_login_compare_status=$?
  [ "$pre_login_compare_status" -eq 0 ] || fail_runtime runtime-version-regressed-from-login
fi

target_real=$(readlink -f "$candidate" 2>/dev/null) || fail_runtime target-canonicalization-failed
case "$target_real" in
  "$release_root"/*/bin/codex | "$release_root"/*/codex)
    verify_direct_release_target "$target_real" || fail_runtime release-identity-unknown
    [ "$verified_release_version" = "$target_version" ] || fail_runtime release-version-mismatch
    mark_verified_release "$verified_release_dir" "$verified_release_version" || fail_runtime release-marker-invalid
    ;;
esac

if [ "$target_changed" = yes ] || [ "$launcher_changed" = yes ]; then
  emit_runtime_result coordinated runtime-coordinated
else
  emit_runtime_result unchanged runtime-already-coordinated
fi
exit 0
"###;

// Cleanup is intentionally conservative: only direct, marked release directories are candidates.
// Process identity uncertainty or a changed directory identity always keeps the release in place.
pub(crate) const REMOTE_CODEX_RELEASE_CLEANUP_SCRIPT: &str = r###"set -u
umask 077
hub_dir="$HOME/.codex-hub"
standalone_root="$HOME/.codex/packages/standalone"
release_root="$standalone_root/releases"
current_link="$standalone_root/current"
target_file="$hub_dir/codex-target"
marker_name=".codexhub-managed-release"
capture_marker_suffix=".codexhub-managed-capture"
cleanup_policy=${codexhub_cleanup_policy:-managed-only}
cleanup_verified_version=${codexhub_cleanup_verified_version:-}
cleanup_backup_mode=${codexhub_cleanup_backup_mode:-none}
proc_root=/proc
owner_process_root=/proc
scanned=0
adopted=0
removed=0
backed_up=0
retained=0
deferred=0
cleanup_reason=cleanup-complete
work_dir="${TMPDIR:-/tmp}/codexhub-release-cleanup.$$"
proc_exes="$work_dir/proc-exes"
ignored_session_processes="$work_dir/ignored-session-processes"
ignored_session_process_count=0
active_candidate=""
active_quarantine=""
active_kind=""
cleanup_lock_root="$HOME"
cleanup_lock="$cleanup_lock_root/.codexhub-runtime-cleanup.lock"
cleanup_lock_held=no
cleanup_lock_uid=""
cleanup_lock_pid=""
cleanup_lock_starttime=""
backup_initialized=no
backup_id=none
backup_root="$hub_dir/deletion-backups"
backup_dir=""
backup_releases=""
backup_captures=""
backup_manifest=""

emit_cleanup_result() {
  printf 'CODEXHUB_CLEANUP_STATUS=%s\n' "$1"
  printf 'CODEXHUB_CLEANUP_SCANNED=%s\n' "$scanned"
  printf 'CODEXHUB_CLEANUP_ADOPTED=%s\n' "$adopted"
  printf 'CODEXHUB_CLEANUP_REMOVED=%s\n' "$removed"
  printf 'CODEXHUB_CLEANUP_BACKED_UP=%s\n' "$backed_up"
  printf 'CODEXHUB_CLEANUP_BACKUP_ID=%s\n' "$backup_id"
  printf 'CODEXHUB_CLEANUP_IGNORED_SESSION_PROCESSES=%s\n' "$ignored_session_process_count"
  printf 'CODEXHUB_CLEANUP_RETAINED=%s\n' "$retained"
  printf 'CODEXHUB_CLEANUP_DEFERRED=%s\n' "$deferred"
  printf 'CODEXHUB_CLEANUP_REASON=%s\n' "$2"
}

fail_cleanup() {
  emit_cleanup_result failed "$1"
  exit 1
}

restore_active_quarantine() {
  if [ -z "$active_candidate" ] || [ -z "$active_quarantine" ]; then
    return 0
  fi
  if [ ! -e "$active_quarantine" ] && [ ! -L "$active_quarantine" ]; then
    active_candidate=""
    active_quarantine=""
    return 0
  fi
  verify_owned_cleanup_lock || return 1
  if [ -e "$active_candidate" ] || [ -L "$active_candidate" ]; then
    return 1
  fi
  active_candidate_name=${active_candidate##*/}
  case "$active_kind" in
    release)
      verify_quarantine "$active_quarantine" "$active_candidate_name" || return 1
      mv -T -n "$active_quarantine" "$active_candidate" 2>/dev/null || return 1
      [ ! -e "$active_quarantine" ] && [ ! -L "$active_quarantine" ] || return 1
      verify_candidate "$active_candidate" || return 1
      ;;
    capture)
      verify_capture_quarantine "$active_quarantine" "$active_candidate_name" || return 1
      mv -T -n "$active_quarantine" "$active_candidate" 2>/dev/null || return 1
      [ ! -e "$active_quarantine" ] && [ ! -L "$active_quarantine" ] || return 1
      verify_capture_candidate "$active_candidate" || return 1
      ;;
    *) return 1 ;;
  esac
  active_candidate=""
  active_quarantine=""
  active_kind=""
  return 0
}

cleanup_work_dir() {
  restore_active_quarantine 2>/dev/null || true
  release_cleanup_lock 2>/dev/null || true
  rm -f "$proc_exes" "$ignored_session_processes"
  rmdir "$work_dir" 2>/dev/null || true
}

cleanup_mv_no_replace_supported() {
  mv_probe_source="$work_dir/mv-source"
  mv_probe_destination="$work_dir/mv-destination"
  mv_probe_move_source="$work_dir/mv-move-source"
  mv_probe_move_destination="$work_dir/mv-move-destination"
  [ ! -e "$mv_probe_source" ] && [ ! -L "$mv_probe_source" ] || return 1
  [ ! -e "$mv_probe_destination" ] && [ ! -L "$mv_probe_destination" ] || return 1
  [ ! -e "$mv_probe_move_source" ] && [ ! -L "$mv_probe_move_source" ] || return 1
  [ ! -e "$mv_probe_move_destination" ] && [ ! -L "$mv_probe_move_destination" ] || return 1
  printf 'source\n' >"$mv_probe_source" || return 1
  printf 'destination\n' >"$mv_probe_destination" || {
    rm -f "$mv_probe_source" 2>/dev/null || true
    return 1
  }
  printf 'move-source\n' >"$mv_probe_move_source" || {
    rm -f "$mv_probe_source" "$mv_probe_destination" 2>/dev/null || true
    return 1
  }
  mv -T -n "$mv_probe_source" "$mv_probe_destination" >/dev/null 2>&1
  mv_probe_status=$?
  mv -T -n "$mv_probe_move_source" "$mv_probe_move_destination" >/dev/null 2>&1
  mv_probe_move_status=$?
  mv_probe_collision_safe=no
  case "$mv_probe_status" in
    0 | 1) mv_probe_collision_safe=yes ;;
  esac
  mv_probe_safe=no
  # Accept coreutils 8.x/9.4 collision statuses only after a positive move.
  if [ "$mv_probe_collision_safe" = yes ] &&
    [ -f "$mv_probe_source" ] && [ ! -L "$mv_probe_source" ] &&
    [ -f "$mv_probe_destination" ] && [ ! -L "$mv_probe_destination" ] &&
    [ "$(wc -c <"$mv_probe_source" 2>/dev/null | tr -d '[:space:]')" = 7 ] &&
    [ "$(wc -c <"$mv_probe_destination" 2>/dev/null | tr -d '[:space:]')" = 12 ] &&
    [ "$(sed -n '1p' "$mv_probe_source" 2>/dev/null)" = source ] &&
    [ "$(sed -n '1p' "$mv_probe_destination" 2>/dev/null)" = destination ] &&
    [ "$mv_probe_move_status" -eq 0 ] &&
    [ ! -e "$mv_probe_move_source" ] && [ ! -L "$mv_probe_move_source" ] &&
    [ -f "$mv_probe_move_destination" ] && [ ! -L "$mv_probe_move_destination" ] &&
    [ "$(wc -c <"$mv_probe_move_destination" 2>/dev/null | tr -d '[:space:]')" = 12 ] &&
    [ "$(sed -n '1p' "$mv_probe_move_destination" 2>/dev/null)" = move-source ]; then
    mv_probe_safe=yes
  fi
  rm -f "$mv_probe_source" "$mv_probe_destination" \
    "$mv_probe_move_source" "$mv_probe_move_destination" 2>/dev/null || mv_probe_safe=no
  [ "$mv_probe_safe" = yes ]
}

for tool in readlink awk sed wc tr od id rm mv mkdir rmdir ln chmod date stat; do
  command -v "$tool" >/dev/null 2>&1 || fail_cleanup required-tool-unavailable
done
[ -d "$cleanup_lock_root" ] && [ ! -L "$cleanup_lock_root" ] || fail_cleanup cleanup-lock-root-identity-unknown
cleanup_lock_root_real=$(readlink -f "$cleanup_lock_root" 2>/dev/null) || fail_cleanup cleanup-lock-root-identity-unknown
[ "$cleanup_lock_root_real" = "$cleanup_lock_root" ] || fail_cleanup cleanup-lock-root-parent-mismatch
mkdir "$work_dir" 2>/dev/null || fail_cleanup work-dir-unavailable
trap cleanup_work_dir EXIT
trap 'trap - EXIT; cleanup_work_dir; exit 129' HUP
trap 'trap - EXIT; cleanup_work_dir; exit 130' INT
trap 'trap - EXIT; cleanup_work_dir; exit 143' TERM
cleanup_mv_no_replace_supported || fail_cleanup mv-no-replace-unavailable
: >"$proc_exes" || fail_cleanup work-file-unavailable
: >"$ignored_session_processes" || fail_cleanup work-file-unavailable

release_root_available=no
release_root_real=""
if [ -e "$release_root" ] || [ -L "$release_root" ]; then
  if [ ! -d "$release_root" ] || [ -L "$release_root" ]; then
    scanned=1
    deferred=1
    emit_cleanup_result deferred release-root-identity-unknown
    exit 0
  fi
  release_root_real=$(readlink -f "$release_root" 2>/dev/null) || {
    scanned=1
    deferred=1
    emit_cleanup_result deferred release-root-identity-unknown
    exit 0
  }
  [ "$release_root_real" = "$release_root" ] || {
    scanned=1
    deferred=1
    emit_cleanup_result deferred release-root-parent-mismatch
    exit 0
  }
  release_root_available=yes
fi

hub_dir_available=no
hub_dir_real=""
if [ ! -e "$hub_dir" ] && [ ! -L "$hub_dir" ]; then
  mkdir "$hub_dir" 2>/dev/null || fail_cleanup hub-dir-create-failed
  chmod 700 "$hub_dir" 2>/dev/null || fail_cleanup hub-dir-permission-failed
fi
if [ -e "$hub_dir" ] || [ -L "$hub_dir" ]; then
  if [ ! -d "$hub_dir" ] || [ -L "$hub_dir" ]; then
    scanned=1
    deferred=1
    emit_cleanup_result deferred hub-dir-identity-unknown
    exit 0
  fi
  hub_dir_real=$(readlink -f "$hub_dir" 2>/dev/null) || {
    scanned=1
    deferred=1
    emit_cleanup_result deferred hub-dir-identity-unknown
    exit 0
  }
  [ "$hub_dir_real" = "$hub_dir" ] || {
    scanned=1
    deferred=1
    emit_cleanup_result deferred hub-dir-parent-mismatch
    exit 0
  }
  hub_dir_available=yes
fi

is_safe_release_entry() {
  value=$1
  case "$value" in
    "" | "." | ".." | */* | *[!A-Za-z0-9._+-]*) return 1 ;;
  esac
  return 0
}

release_entry_matches_version() {
  release_entry=$1
  binary_version=$2
  case "$release_entry" in
    "$binary_version") return 0 ;;
    "$binary_version"-*)
      vendor_suffix=${release_entry#"$binary_version"-}
      is_safe_release_entry "$vendor_suffix"
      return $?
      ;;
    *) return 1 ;;
  esac
}

managed_marker_valid() {
  release_dir=$1
  release_version=$2
  marker="$release_dir/$marker_name"
  [ -f "$marker" ] && [ ! -L "$marker" ] || return 1
  [ "$(wc -l <"$marker" 2>/dev/null | tr -d '[:space:]')" = 2 ] || return 1
  [ "$(sed -n '1p' "$marker" 2>/dev/null)" = "CodexHub managed standalone release v1" ] || return 1
  [ "$(sed -n '2p' "$marker" 2>/dev/null)" = "version=$release_version" ] || return 1
}

normalized_version_for_binary() {
  binary=$1
  [ -f "$binary" ] && [ -x "$binary" ] && [ ! -L "$binary" ] || return 1
  raw_version=$("$binary" --version 2>/dev/null) || return 1
  printf '%s\n' "$raw_version" | awk '
    NF { count += 1; value = $NF }
    END {
      sub(/^v/, "", value)
      if (count != 1 || value !~ /^[0-9A-Za-z.+-]+$/) exit 1
      split(value, build_parts, "+")
      split(build_parts[1], prerelease_parts, "-")
      number_count = split(prerelease_parts[1], numbers, ".")
      if (number_count < 2 || number_count > 4) exit 1
      for (number_index = 1; number_index <= number_count; number_index += 1) {
        if (numbers[number_index] !~ /^[0-9]+$/) exit 1
      }
      print value
    }
  '
}

# Returns 0 only when the first verified Codex version is strictly lower.
# Invalid or incomparable values return 2 so callers retain the release.
version_is_strictly_lower() {
  lower_version=$1
  upper_version=$2
  LC_ALL=C awk -v lower="$lower_version" -v upper="$upper_version" '
    function parse(value, metadata, numbers, plus_position, dash_position, number_count, part_index) {
      sub(/^v/, "", value)
      plus_position = index(value, "+")
      if (plus_position > 0) value = substr(value, 1, plus_position - 1)
      dash_position = index(value, "-")
      if (dash_position > 0) {
        metadata["prerelease"] = substr(value, dash_position + 1)
        value = substr(value, 1, dash_position - 1)
      } else {
        metadata["prerelease"] = ""
      }
      number_count = split(value, numbers, ".")
      if (number_count < 2 || number_count > 4) return 0
      for (part_index = 1; part_index <= 4; part_index += 1) {
        if (part_index > number_count) numbers[part_index] = 0
        if (numbers[part_index] !~ /^[0-9]+$/) return 0
      }
      return 1
    }
    BEGIN {
      if (!parse(lower, lower_metadata, lower_numbers) ||
          !parse(upper, upper_metadata, upper_numbers)) exit 2
      for (part_index = 1; part_index <= 4; part_index += 1) {
        if ((lower_numbers[part_index] + 0) < (upper_numbers[part_index] + 0)) exit 0
        if ((lower_numbers[part_index] + 0) > (upper_numbers[part_index] + 0)) exit 1
      }
      lower_prerelease = lower_metadata["prerelease"]
      upper_prerelease = upper_metadata["prerelease"]
      if (lower_prerelease == "" && upper_prerelease == "") exit 1
      if (lower_prerelease != "" && upper_prerelease == "") exit 0
      if (lower_prerelease == "" && upper_prerelease != "") exit 1
      lower_count = split(lower_prerelease, lower_identifiers, ".")
      upper_count = split(upper_prerelease, upper_identifiers, ".")
      identifier_count = lower_count > upper_count ? lower_count : upper_count
      for (part_index = 1; part_index <= identifier_count; part_index += 1) {
        if (part_index > lower_count) exit 0
        if (part_index > upper_count) exit 1
        lower_identifier = lower_identifiers[part_index]
        upper_identifier = upper_identifiers[part_index]
        lower_numeric = lower_identifier ~ /^[0-9]+$/
        upper_numeric = upper_identifier ~ /^[0-9]+$/
        if (lower_numeric && upper_numeric) {
          if ((lower_identifier + 0) < (upper_identifier + 0)) exit 0
          if ((lower_identifier + 0) > (upper_identifier + 0)) exit 1
        } else if (lower_numeric != upper_numeric) {
          exit lower_numeric ? 0 : 1
        } else {
          if (lower_identifier < upper_identifier) exit 0
          if (lower_identifier > upper_identifier) exit 1
        }
      }
      exit 1
    }
  '
}

read_process_identity() {
  identity_pid=$1
  observed_process_uid=""
  observed_process_starttime=""
  case "$identity_pid" in "" | *[!0-9]*) return 2 ;; esac
  identity_proc_dir="$owner_process_root/$identity_pid"
  [ -d "$identity_proc_dir" ] || return 1
  if [ ! -r "$identity_proc_dir/status" ] || [ ! -r "$identity_proc_dir/stat" ]; then
    [ -d "$identity_proc_dir" ] && return 2
    return 1
  fi
  observed_process_uid=$(awk '/^Uid:/ { print $2; exit }' "$identity_proc_dir/status" 2>/dev/null)
  [ -n "$observed_process_uid" ] || { [ -d "$identity_proc_dir" ] && return 2; return 1; }
  identity_stat=$(sed -n '1p' "$identity_proc_dir/stat" 2>/dev/null) || return 2
  identity_after_comm=${identity_stat##*) }
  [ "$identity_after_comm" != "$identity_stat" ] || return 2
  observed_process_starttime=$(printf '%s\n' "$identity_after_comm" | awk '{ print $20 }')
  case "$observed_process_starttime" in "" | *[!0-9]*) return 2 ;; esac
  return 0
}

verify_cleanup_lock_file() {
  lock_path=$1
  lock_kind=$2
  verified_lock_uid=""
  verified_lock_pid=""
  verified_lock_starttime=""
  [ "$hub_dir_available" = yes ] || return 1
  lock_name=${lock_path##*/}
  case "$lock_kind:$lock_name" in
    fixed:.codexhub-runtime-cleanup.lock) ;;
    candidate:.codexhub-runtime-cleanup.owner.*.*)
      lock_numbers=${lock_name#".codexhub-runtime-cleanup.owner."}
      lock_number_one=${lock_numbers%%.*}
      lock_number_two=${lock_numbers#"$lock_number_one."}
      case "$lock_number_one:$lock_number_two" in *[!0-9:]* | :* | *:) return 1 ;; esac
      ;;
    stale:.codexhub-runtime-cleanup.lock.stale.*.*)
      lock_numbers=${lock_name#".codexhub-runtime-cleanup.lock.stale."}
      lock_number_one=${lock_numbers%%.*}
      lock_number_two=${lock_numbers#"$lock_number_one."}
      case "$lock_number_one:$lock_number_two" in *[!0-9:]* | :* | *:) return 1 ;; esac
      ;;
    *) return 1 ;;
  esac
  [ -f "$lock_path" ] && [ ! -L "$lock_path" ] || return 1
  lock_parent_real=$(readlink -f "${lock_path%/*}" 2>/dev/null) || return 1
  lock_real=$(readlink -f "$lock_path" 2>/dev/null) || return 1
  [ "$lock_parent_real" = "$cleanup_lock_root_real" ] || return 1
  [ "$lock_real" = "$cleanup_lock_root_real/$lock_name" ] || return 1
  [ "$(wc -l <"$lock_path" 2>/dev/null | tr -d '[:space:]')" = 4 ] || return 1
  [ "$(sed -n '1p' "$lock_path" 2>/dev/null)" = "CodexHub runtime cleanup lock v1" ] || return 1
  lock_uid_line=$(sed -n '2p' "$lock_path" 2>/dev/null) || return 1
  lock_pid_line=$(sed -n '3p' "$lock_path" 2>/dev/null) || return 1
  lock_starttime_line=$(sed -n '4p' "$lock_path" 2>/dev/null) || return 1
  case "$lock_uid_line" in uid=*) lock_uid=${lock_uid_line#uid=} ;; *) return 1 ;; esac
  case "$lock_pid_line" in pid=*) lock_pid=${lock_pid_line#pid=} ;; *) return 1 ;; esac
  case "$lock_starttime_line" in starttime=*) lock_starttime=${lock_starttime_line#starttime=} ;; *) return 1 ;; esac
  case "$lock_uid:$lock_pid:$lock_starttime" in *[!0-9:]* | :* | *: | *::* ) return 1 ;; esac
  verified_lock_uid=$lock_uid
  verified_lock_pid=$lock_pid
  verified_lock_starttime=$lock_starttime
  return 0
}

cleanup_lock_owner_activity() {
  owner_uid=$1
  owner_pid=$2
  owner_starttime=$3
  [ "$owner_uid" = "$cleanup_lock_uid" ] || return 2
  read_process_identity "$owner_pid"
  identity_status=$?
  case "$identity_status" in
    0)
      if [ "$observed_process_uid" = "$owner_uid" ] && [ "$observed_process_starttime" = "$owner_starttime" ]; then
        return 0
      fi
      return 1
      ;;
    1) return 1 ;;
    *) return 2 ;;
  esac
}

prepare_cleanup_lock_candidate() {
  cleanup_lock_uid=$(id -u 2>/dev/null) || return 1
  cleanup_lock_pid=$$
  read_process_identity "$cleanup_lock_pid" || return 1
  [ "$observed_process_uid" = "$cleanup_lock_uid" ] || return 1
  cleanup_lock_starttime=$observed_process_starttime
  cleanup_lock_candidate="$cleanup_lock_root/.codexhub-runtime-cleanup.owner.$cleanup_lock_pid.$cleanup_lock_starttime"
  [ ! -e "$cleanup_lock_candidate" ] && [ ! -L "$cleanup_lock_candidate" ] || return 1
  if ! {
    printf 'CodexHub runtime cleanup lock v1\n'
    printf 'uid=%s\n' "$cleanup_lock_uid"
    printf 'pid=%s\n' "$cleanup_lock_pid"
    printf 'starttime=%s\n' "$cleanup_lock_starttime"
  } >"$cleanup_lock_candidate" || ! chmod 600 "$cleanup_lock_candidate"; then
    rm -f "$cleanup_lock_candidate" 2>/dev/null || true
    return 1
  fi
  verify_cleanup_lock_file "$cleanup_lock_candidate" candidate || return 1
  [ "$verified_lock_uid" = "$cleanup_lock_uid" ] || return 1
  [ "$verified_lock_pid" = "$cleanup_lock_pid" ] || return 1
  [ "$verified_lock_starttime" = "$cleanup_lock_starttime" ] || return 1
  return 0
}

verify_owned_cleanup_lock() {
  [ "$cleanup_lock_held" = yes ] || return 1
  verify_cleanup_lock_file "$cleanup_lock" fixed || return 1
  [ "$verified_lock_uid" = "$cleanup_lock_uid" ] || return 1
  [ "$verified_lock_pid" = "$cleanup_lock_pid" ] || return 1
  [ "$verified_lock_starttime" = "$cleanup_lock_starttime" ] || return 1
  cleanup_lock_owner_activity "$verified_lock_uid" "$verified_lock_pid" "$verified_lock_starttime"
  [ "$?" -eq 0 ]
}

remove_stale_cleanup_locks() {
  verify_owned_cleanup_lock || return 2
  for stale_lock in "$cleanup_lock_root"/.codexhub-runtime-cleanup.lock.stale.*; do
    if [ ! -e "$stale_lock" ] && [ ! -L "$stale_lock" ]; then
      continue
    fi
    verify_cleanup_lock_file "$stale_lock" stale || return 2
    stale_uid=$verified_lock_uid
    stale_pid=$verified_lock_pid
    stale_starttime=$verified_lock_starttime
    cleanup_lock_owner_activity "$stale_uid" "$stale_pid" "$stale_starttime"
    stale_activity=$?
    case "$stale_activity" in 0) return 3 ;; 1) ;; *) return 2 ;; esac
    verify_owned_cleanup_lock || return 2
    verify_cleanup_lock_file "$stale_lock" stale || return 2
    [ "$verified_lock_uid" = "$stale_uid" ] || return 2
    [ "$verified_lock_pid" = "$stale_pid" ] || return 2
    [ "$verified_lock_starttime" = "$stale_starttime" ] || return 2
    cleanup_lock_owner_activity "$stale_uid" "$stale_pid" "$stale_starttime"
    [ "$?" -eq 1 ] || return 2
    rm -f "$stale_lock" || return 2
    [ ! -e "$stale_lock" ] && [ ! -L "$stale_lock" ] || return 2
  done
  return 0
}

acquire_cleanup_lock() {
  [ "$hub_dir_available" = yes ] || return 2
  prepare_cleanup_lock_candidate || return 4
  if ln "$cleanup_lock_candidate" "$cleanup_lock" 2>/dev/null; then
    cleanup_lock_held=yes
    rm -f "$cleanup_lock_candidate" || return 4
    verify_owned_cleanup_lock || return 4
    remove_stale_cleanup_locks
    stale_cleanup_status=$?
    [ "$stale_cleanup_status" -eq 0 ] || return "$stale_cleanup_status"
    return 0
  fi
  rm -f "$cleanup_lock_candidate" 2>/dev/null || return 4
  verify_cleanup_lock_file "$cleanup_lock" fixed || return 2
  existing_lock_uid=$verified_lock_uid
  existing_lock_pid=$verified_lock_pid
  existing_lock_starttime=$verified_lock_starttime
  cleanup_lock_owner_activity "$existing_lock_uid" "$existing_lock_pid" "$existing_lock_starttime"
  existing_activity=$?
  case "$existing_activity" in
    0) return 3 ;;
    1) ;;
    *) return 2 ;;
  esac
  stale_lock="$cleanup_lock_root/.codexhub-runtime-cleanup.lock.stale.$cleanup_lock_pid.$cleanup_lock_starttime"
  [ ! -e "$stale_lock" ] && [ ! -L "$stale_lock" ] || return 2
  verify_cleanup_lock_file "$cleanup_lock" fixed || return 2
  [ "$verified_lock_uid" = "$existing_lock_uid" ] || return 2
  [ "$verified_lock_pid" = "$existing_lock_pid" ] || return 2
  [ "$verified_lock_starttime" = "$existing_lock_starttime" ] || return 2
  cleanup_lock_owner_activity "$existing_lock_uid" "$existing_lock_pid" "$existing_lock_starttime"
  [ "$?" -eq 1 ] || return 2
  mv -T -n "$cleanup_lock" "$stale_lock" 2>/dev/null || return 2
  [ ! -e "$cleanup_lock" ] && [ ! -L "$cleanup_lock" ] || return 2
  verify_cleanup_lock_file "$stale_lock" stale || return 2
  [ "$verified_lock_uid" = "$existing_lock_uid" ] || return 2
  [ "$verified_lock_pid" = "$existing_lock_pid" ] || return 2
  [ "$verified_lock_starttime" = "$existing_lock_starttime" ] || return 2
  cleanup_lock_owner_activity "$existing_lock_uid" "$existing_lock_pid" "$existing_lock_starttime"
  [ "$?" -eq 1 ] || return 2
  prepare_cleanup_lock_candidate || return 4
  if ! ln "$cleanup_lock_candidate" "$cleanup_lock" 2>/dev/null; then
    rm -f "$cleanup_lock_candidate" 2>/dev/null || true
    return 3
  fi
  cleanup_lock_held=yes
  rm -f "$cleanup_lock_candidate" || return 4
  verify_owned_cleanup_lock || return 4
  remove_stale_cleanup_locks
  stale_cleanup_status=$?
  [ "$stale_cleanup_status" -eq 0 ] || return "$stale_cleanup_status"
  return 0
}

release_cleanup_lock() {
  [ "$cleanup_lock_held" = yes ] || return 0
  verify_owned_cleanup_lock || return 1
  rm -f "$cleanup_lock" || return 1
  [ ! -e "$cleanup_lock" ] && [ ! -L "$cleanup_lock" ] || return 1
  cleanup_lock_held=no
  return 0
}

is_safe_capture_name() {
  value=$1
  case "$value" in codex-original.*.*) ;; *) return 1 ;; esac
  capture_numbers=${value#codex-original.}
  capture_nonce=${capture_numbers##*.}
  capture_timestamp=${capture_numbers%".$capture_nonce"}
  [ "$capture_numbers" = "$capture_timestamp.$capture_nonce" ] || return 1
  case "$capture_timestamp" in "" | *[!0-9]*) return 1 ;; esac
  case "$capture_nonce" in "" | *[!0-9]*) return 1 ;; esac
  return 0
}

capture_marker_valid() {
  capture_name=$1
  capture_version=$2
  marker="$hub_dir/$capture_name$capture_marker_suffix"
  [ -f "$marker" ] && [ ! -L "$marker" ] || return 1
  marker_parent_real=$(readlink -f "${marker%/*}" 2>/dev/null) || return 1
  marker_real=$(readlink -f "$marker" 2>/dev/null) || return 1
  [ "$marker_parent_real" = "$hub_dir_real" ] || return 1
  [ "$marker_real" = "$hub_dir_real/$capture_name$capture_marker_suffix" ] || return 1
  [ "$(wc -l <"$marker" 2>/dev/null | tr -d '[:space:]')" = 3 ] || return 1
  [ "$(sed -n '1p' "$marker" 2>/dev/null)" = "CodexHub managed launcher capture v1" ] || return 1
  [ "$(sed -n '2p' "$marker" 2>/dev/null)" = "name=$capture_name" ] || return 1
  [ "$(sed -n '3p' "$marker" 2>/dev/null)" = "version=$capture_version" ] || return 1
  return 0
}

verify_capture_candidate() {
  capture=$1
  verified_capture_real=""
  verified_capture_name=""
  verified_capture_version=""
  [ "$hub_dir_available" = yes ] || return 1
  capture_name=${capture##*/}
  is_safe_capture_name "$capture_name" || return 1
  [ "$capture" = "$hub_dir/$capture_name" ] || return 1
  [ -f "$capture" ] && [ -x "$capture" ] && [ ! -L "$capture" ] || return 1
  capture_parent_real=$(readlink -f "${capture%/*}" 2>/dev/null) || return 1
  capture_real=$(readlink -f "$capture" 2>/dev/null) || return 1
  [ "$capture_parent_real" = "$hub_dir_real" ] || return 1
  [ "$capture_real" = "$hub_dir_real/$capture_name" ] || return 1
  capture_version=$(normalized_version_for_binary "$capture" 2>/dev/null) || return 1
  capture_marker_valid "$capture_name" "$capture_version" || return 1
  verified_capture_real=$capture_real
  verified_capture_name=$capture_name
  verified_capture_version=$capture_version
  return 0
}

verify_capture_quarantine() {
  quarantine=$1
  expected_name=$2
  verified_capture_quarantine_real=""
  verified_capture_quarantine_version=""
  [ "$hub_dir_available" = yes ] || return 1
  is_safe_capture_name "$expected_name" || return 1
  quarantine_name=${quarantine##*/}
  quarantine_prefix=".codexhub-capture-quarantine.$expected_name."
  case "$quarantine_name" in
    "$quarantine_prefix"*) quarantine_nonce=${quarantine_name#"$quarantine_prefix"} ;;
    *) return 1 ;;
  esac
  case "$quarantine_nonce" in "" | *[!0-9]*) return 1 ;; esac
  [ -f "$quarantine" ] && [ -x "$quarantine" ] && [ ! -L "$quarantine" ] || return 1
  quarantine_parent_real=$(readlink -f "${quarantine%/*}" 2>/dev/null) || return 1
  quarantine_real=$(readlink -f "$quarantine" 2>/dev/null) || return 1
  [ "$quarantine_parent_real" = "$hub_dir_real" ] || return 1
  [ "$quarantine_real" = "$hub_dir_real/$quarantine_name" ] || return 1
  quarantine_version=$(normalized_version_for_binary "$quarantine" 2>/dev/null) || return 1
  capture_marker_valid "$expected_name" "$quarantine_version" || return 1
  verified_capture_quarantine_real=$quarantine_real
  verified_capture_quarantine_version=$quarantine_version
  return 0
}

select_release_binary() {
  release_dir=$1
  release_entry=$2
  selected_release_binary=""
  selected_release_binary_real=""
  selected_release_binary_relative_path=""
  selected_release_version=""
  is_safe_release_entry "$release_entry" || return 1
  [ -d "$release_dir" ] && [ ! -L "$release_dir" ] || return 1
  release_dir_real=$(readlink -f "$release_dir" 2>/dev/null) || return 1
  release_dir_basename=${release_dir##*/}
  is_safe_release_entry "$release_dir_basename" || return 1
  [ "$release_dir_real" = "$release_root_real/$release_dir_basename" ] || return 1
  release_binary_match_count=0
  for binary_relative_path in bin/codex codex; do
    binary="$release_dir/$binary_relative_path"
    if [ -e "$binary" ] || [ -L "$binary" ]; then
      release_binary_match_count=$((release_binary_match_count + 1))
      [ "$release_binary_match_count" -eq 1 ] || return 1
      saved_release_binary=$binary
      saved_release_binary_relative_path=$binary_relative_path
    fi
  done
  [ "$release_binary_match_count" -eq 1 ] || return 1
  [ -f "$saved_release_binary" ] && [ -x "$saved_release_binary" ] &&
    [ ! -L "$saved_release_binary" ] || return 1
  binary_real=$(readlink -f "$saved_release_binary" 2>/dev/null) || return 1
  [ "$binary_real" = "$release_dir_real/$saved_release_binary_relative_path" ] || return 1
  binary_version=$(normalized_version_for_binary "$saved_release_binary" 2>/dev/null) || return 1
  release_entry_matches_version "$release_entry" "$binary_version" || return 1
  selected_release_binary=$saved_release_binary
  selected_release_binary_real=$binary_real
  selected_release_binary_relative_path=$saved_release_binary_relative_path
  selected_release_version=$binary_version
  return 0
}

verify_release_identity() {
  candidate=$1
  [ -d "$candidate" ] && [ ! -L "$candidate" ] || return 1
  candidate_name=${candidate##*/}
  is_safe_release_entry "$candidate_name" || return 1
  parent_real=$(readlink -f "${candidate%/*}" 2>/dev/null) || return 1
  candidate_real=$(readlink -f "$candidate" 2>/dev/null) || return 1
  [ "$parent_real" = "$release_root_real" ] || return 1
  [ "$candidate_real" = "$release_root_real/$candidate_name" ] || return 1
  select_release_binary "$candidate" "$candidate_name" || return 1
  verified_candidate_real=$candidate_real
  verified_candidate_name=$candidate_name
  verified_candidate_version=$selected_release_version
  verified_candidate_binary_relative_path=$selected_release_binary_relative_path
  return 0
}

verify_candidate() {
  candidate=$1
  verify_release_identity "$candidate" || return 1
  managed_marker_valid "$candidate" "$verified_candidate_version" || return 1
  return 0
}

# Update can take ownership only after the complete release identity and lower
# version relation have been rechecked while holding the shared writer lock.
adopt_verified_release() {
  adoption_candidate=$1
  expected_real=$2
  expected_name=$3
  expected_version=$4
  marker="$adoption_candidate/$marker_name"
  if [ -e "$marker" ] || [ -L "$marker" ]; then
    managed_marker_valid "$adoption_candidate" "$expected_version"
    return $?
  fi
  [ "$cleanup_policy" = verified-older-than ] || return 1
  verify_owned_cleanup_lock || return 2
  verify_release_identity "$adoption_candidate" || return 2
  [ "$verified_candidate_real" = "$expected_real" ] || return 2
  [ "$verified_candidate_name" = "$expected_name" ] || return 2
  [ "$verified_candidate_version" = "$expected_version" ] || return 2
  version_is_strictly_lower "$expected_version" "$cleanup_verified_version"
  [ "$?" -eq 0 ] || return 2
  marker_tmp="$adoption_candidate/$marker_name.tmp.$$"
  [ ! -e "$marker_tmp" ] && [ ! -L "$marker_tmp" ] || return 2
  if ! {
    printf 'CodexHub managed standalone release v1\n'
    printf 'version=%s\n' "$expected_version"
  } >"$marker_tmp" || ! chmod 600 "$marker_tmp"; then
    rm -f "$marker_tmp" 2>/dev/null || true
    return 2
  fi
  if ! ln "$marker_tmp" "$marker" 2>/dev/null; then
    rm -f "$marker_tmp" 2>/dev/null || true
    managed_marker_valid "$adoption_candidate" "$expected_version" || return 2
    return 0
  fi
  adopted=$((adopted + 1))
  rm -f "$marker_tmp" || return 2
  managed_marker_valid "$adoption_candidate" "$expected_version" || return 2
  return 0
}

verify_quarantine() {
  quarantine=$1
  expected_name=$2
  [ -d "$quarantine" ] && [ ! -L "$quarantine" ] || return 1
  is_safe_release_entry "$expected_name" || return 1
  quarantine_name=${quarantine##*/}
  quarantine_prefix=".codexhub-quarantine.$expected_name."
  case "$quarantine_name" in
    "$quarantine_prefix"*) quarantine_nonce=${quarantine_name#"$quarantine_prefix"} ;;
    *) return 1 ;;
  esac
  case "$quarantine_nonce" in "" | *[!0-9]*) return 1 ;; esac
  quarantine_parent_real=$(readlink -f "${quarantine%/*}" 2>/dev/null) || return 1
  quarantine_real=$(readlink -f "$quarantine" 2>/dev/null) || return 1
  [ "$quarantine_parent_real" = "$release_root_real" ] || return 1
  [ "$quarantine_real" = "$release_root_real/$quarantine_name" ] || return 1
  select_release_binary "$quarantine" "$expected_name" || return 1
  managed_marker_valid "$quarantine" "$selected_release_version" || return 1
  verified_quarantine_real=$quarantine_real
  verified_quarantine_version=$selected_release_version
  verified_quarantine_binary_relative_path=$selected_release_binary_relative_path
  return 0
}

# Update cleanup keeps a reversible, same-filesystem backup instead of
# permanently deleting a verified old runtime during the button workflow.
ensure_update_backup() {
  [ "$cleanup_backup_mode" = staged ] || return 2
  [ "$cleanup_policy" = verified-older-than ] || return 2
  [ "$backup_initialized" = no ] || return 0
  verify_owned_cleanup_lock || return 2
  backup_timestamp=$(date -u '+%Y%m%d%H%M%S' 2>/dev/null) || return 2
  case "$backup_timestamp" in *[!0-9]* | "") return 2 ;; esac
  [ "${#backup_timestamp}" -eq 14 ] || return 2
  backup_id="update-$backup_timestamp-$$"
  case "$backup_id" in *[!a-zA-Z0-9-]*) return 2 ;; esac

  if [ ! -e "$backup_root" ] && [ ! -L "$backup_root" ]; then
    mkdir "$backup_root" 2>/dev/null || return 2
    chmod 700 "$backup_root" 2>/dev/null || return 2
  fi
  [ -d "$backup_root" ] && [ ! -L "$backup_root" ] || return 2
  backup_root_parent_real=$(readlink -f "${backup_root%/*}" 2>/dev/null) || return 2
  backup_root_real=$(readlink -f "$backup_root" 2>/dev/null) || return 2
  [ "$backup_root_parent_real" = "$hub_dir_real" ] || return 2
  [ "$backup_root_real" = "$hub_dir_real/deletion-backups" ] || return 2
  [ "$(stat -c '%u' "$backup_root" 2>/dev/null)" = "$(id -u 2>/dev/null)" ] || return 2
  [ "$(stat -c '%a' "$backup_root" 2>/dev/null)" = 700 ] || return 2

  backup_dir="$backup_root/$backup_id"
  [ ! -e "$backup_dir" ] && [ ! -L "$backup_dir" ] || return 2
  mkdir "$backup_dir" 2>/dev/null || return 2
  chmod 700 "$backup_dir" 2>/dev/null || return 2
  backup_dir_real=$(readlink -f "$backup_dir" 2>/dev/null) || return 2
  [ "$backup_dir_real" = "$backup_root_real/$backup_id" ] || return 2

  backup_releases="$backup_dir/releases"
  backup_captures="$backup_dir/captures"
  backup_links="$backup_dir/links"
  backup_local_links="$backup_links/local-bin"
  backup_tmp_links="$backup_links/codex-tmp-arg0"
  mkdir "$backup_releases" "$backup_captures" "$backup_links" 2>/dev/null || return 2
  mkdir "$backup_local_links" "$backup_tmp_links" 2>/dev/null || return 2
  chmod 700 "$backup_releases" "$backup_captures" "$backup_links" \
    "$backup_local_links" "$backup_tmp_links" 2>/dev/null || return 2
  backup_releases_real=$(readlink -f "$backup_releases" 2>/dev/null) || return 2
  backup_captures_real=$(readlink -f "$backup_captures" 2>/dev/null) || return 2
  backup_local_links_real=$(readlink -f "$backup_local_links" 2>/dev/null) || return 2
  backup_tmp_links_real=$(readlink -f "$backup_tmp_links" 2>/dev/null) || return 2
  [ "$backup_releases_real" = "$backup_dir_real/releases" ] || return 2
  [ "$backup_captures_real" = "$backup_dir_real/captures" ] || return 2
  [ "$backup_local_links_real" = "$backup_dir_real/links/local-bin" ] || return 2
  [ "$backup_tmp_links_real" = "$backup_dir_real/links/codex-tmp-arg0" ] || return 2

  backup_device=$(stat -c '%d' "$backup_dir" 2>/dev/null) || return 2
  hub_device=$(stat -c '%d' "$hub_dir_real" 2>/dev/null) || return 2
  [ "$backup_device" = "$hub_device" ] || return 2
  if [ "$release_root_available" = yes ]; then
    release_device=$(stat -c '%d' "$release_root_real" 2>/dev/null) || return 2
    [ "$backup_device" = "$release_device" ] || return 2
  fi

  backup_manifest="$backup_dir/manifest.txt"
  {
    printf 'CodexHub update cleanup backup v1\n'
    printf 'id=%s\n' "$backup_id"
    printf 'verifiedVersion=%s\n' "$cleanup_verified_version"
  } >"$backup_manifest" || return 2
  chmod 600 "$backup_manifest" 2>/dev/null || return 2
  backup_initialized=yes
  return 0
}

verify_backup_release() {
  backup_candidate=$1
  expected_name=$2
  verified_backup_release_version=""
  [ "$backup_initialized" = yes ] || return 1
  is_safe_release_entry "$expected_name" || return 1
  [ "$backup_candidate" = "$backup_releases/$expected_name" ] || return 1
  [ -d "$backup_candidate" ] && [ ! -L "$backup_candidate" ] || return 1
  backup_candidate_parent_real=$(readlink -f "${backup_candidate%/*}" 2>/dev/null) || return 1
  backup_candidate_real=$(readlink -f "$backup_candidate" 2>/dev/null) || return 1
  [ "$backup_candidate_parent_real" = "$backup_releases_real" ] || return 1
  [ "$backup_candidate_real" = "$backup_releases_real/$expected_name" ] || return 1
  backup_binary_count=0
  for backup_binary_relative in bin/codex codex; do
    backup_binary="$backup_candidate/$backup_binary_relative"
    if [ -e "$backup_binary" ] || [ -L "$backup_binary" ]; then
      backup_binary_count=$((backup_binary_count + 1))
      [ "$backup_binary_count" -eq 1 ] || return 1
      selected_backup_binary=$backup_binary
      selected_backup_binary_relative=$backup_binary_relative
    fi
  done
  [ "$backup_binary_count" -eq 1 ] || return 1
  [ -f "$selected_backup_binary" ] && [ -x "$selected_backup_binary" ] &&
    [ ! -L "$selected_backup_binary" ] || return 1
  selected_backup_binary_real=$(readlink -f "$selected_backup_binary" 2>/dev/null) || return 1
  [ "$selected_backup_binary_real" = "$backup_candidate_real/$selected_backup_binary_relative" ] || return 1
  backup_version=$(normalized_version_for_binary "$selected_backup_binary" 2>/dev/null) || return 1
  release_entry_matches_version "$expected_name" "$backup_version" || return 1
  managed_marker_valid "$backup_candidate" "$backup_version" || return 1
  verified_backup_release_version=$backup_version
  return 0
}

verify_backup_capture() {
  backup_capture=$1
  expected_name=$2
  expected_version=$3
  [ "$backup_initialized" = yes ] || return 1
  is_safe_capture_name "$expected_name" || return 1
  [ "$backup_capture" = "$backup_captures/$expected_name" ] || return 1
  [ -f "$backup_capture" ] && [ -x "$backup_capture" ] && [ ! -L "$backup_capture" ] || return 1
  backup_capture_parent_real=$(readlink -f "${backup_capture%/*}" 2>/dev/null) || return 1
  backup_capture_real=$(readlink -f "$backup_capture" 2>/dev/null) || return 1
  [ "$backup_capture_parent_real" = "$backup_captures_real" ] || return 1
  [ "$backup_capture_real" = "$backup_captures_real/$expected_name" ] || return 1
  [ "$(normalized_version_for_binary "$backup_capture" 2>/dev/null)" = "$expected_version" ] || return 1
  backup_marker="$backup_capture$capture_marker_suffix"
  [ -f "$backup_marker" ] && [ ! -L "$backup_marker" ] || return 1
  [ "$(wc -l <"$backup_marker" 2>/dev/null | tr -d '[:space:]')" = 3 ] || return 1
  [ "$(sed -n '1p' "$backup_marker" 2>/dev/null)" = "CodexHub managed launcher capture v1" ] || return 1
  [ "$(sed -n '2p' "$backup_marker" 2>/dev/null)" = "name=$expected_name" ] || return 1
  [ "$(sed -n '3p' "$backup_marker" 2>/dev/null)" = "version=$expected_version" ] || return 1
  return 0
}

backup_residual_links_for_release() {
  original_release=$1
  isolated_release=$2
  release_name=$3
  [ "$backup_initialized" = yes ] || return 2
  [ ! -e "$original_release" ] && [ ! -L "$original_release" ] || return 2
  for residual_link in \
    "$HOME/.local/bin"/codex.codexhub.bak.* \
    "$HOME/.codex/tmp/arg0"/codex-arg0*/*; do
    [ -L "$residual_link" ] || continue
    residual_target=$(readlink "$residual_link" 2>/dev/null) || return 2
    case "$residual_target" in
      "$original_release" | "$original_release"/* | "$isolated_release" | "$isolated_release"/*) ;;
      *) continue ;;
    esac
    residual_name=${residual_link##*/}
    is_safe_release_entry "$residual_name" || return 2
    case "$residual_link" in
      "$HOME/.local/bin"/*)
        residual_backup_parent="$backup_local_links"
        residual_backup="$backup_local_links/$residual_name"
        expected_backup_parent_real=$backup_local_links_real
        ;;
      "$HOME/.codex/tmp/arg0"/*)
        residual_parent=${residual_link%/*}
        residual_parent_name=${residual_parent##*/}
        is_safe_release_entry "$residual_parent_name" || return 2
        residual_backup_parent="$backup_tmp_links/$residual_parent_name"
        if [ ! -e "$residual_backup_parent" ] && [ ! -L "$residual_backup_parent" ]; then
          mkdir "$residual_backup_parent" 2>/dev/null || return 2
          chmod 700 "$residual_backup_parent" 2>/dev/null || return 2
        fi
        [ -d "$residual_backup_parent" ] && [ ! -L "$residual_backup_parent" ] || return 2
        residual_backup="$residual_backup_parent/$residual_name"
        expected_backup_parent_real="$backup_tmp_links_real/$residual_parent_name"
        ;;
      *) return 2 ;;
    esac
    residual_backup_parent_real=$(readlink -f "$residual_backup_parent" 2>/dev/null) || return 2
    [ "$residual_backup_parent_real" = "$expected_backup_parent_real" ] || return 2
    [ ! -e "$residual_backup" ] && [ ! -L "$residual_backup" ] || return 2
    # A concurrently recreated release must keep every newly relevant link.
    [ ! -e "$original_release" ] && [ ! -L "$original_release" ] || return 2
    mv -T -n "$residual_link" "$residual_backup" 2>/dev/null || return 2
    [ ! -e "$residual_link" ] && [ ! -L "$residual_link" ] && [ -L "$residual_backup" ] || return 2
    [ "$(readlink "$residual_backup" 2>/dev/null)" = "$residual_target" ] || return 2
    printf 'link=%s|release=%s\n' "$residual_backup" "$release_name" >>"$backup_manifest" || return 2
  done
  [ ! -e "$original_release" ] && [ ! -L "$original_release" ] || return 2
  return 0
}

# A previous shell can be interrupted after isolation. Restore only a strict,
# marked quarantine to its exact version path so the normal pass can retry it.
recover_orphaned_quarantines() {
  [ "$release_root_available" = yes ] || return 0
  for quarantine in "$release_root"/.codexhub-quarantine.*; do
    if [ ! -e "$quarantine" ] && [ ! -L "$quarantine" ]; then
      continue
    fi
    quarantine_name=${quarantine##*/}
    quarantine_suffix=${quarantine_name#".codexhub-quarantine."}
    quarantine_nonce=${quarantine_suffix##*.}
    case "$quarantine_nonce" in "" | *[!0-9]*) return 2 ;; esac
    quarantine_entry=${quarantine_suffix%".$quarantine_nonce"}
    [ "$quarantine_suffix" = "$quarantine_entry.$quarantine_nonce" ] || return 2
    is_safe_release_entry "$quarantine_entry" || return 2
    verify_quarantine "$quarantine" "$quarantine_entry" || return 2
    recovered_candidate="$release_root/$quarantine_entry"
    if [ -e "$recovered_candidate" ] || [ -L "$recovered_candidate" ]; then
      return 2
    fi
    verify_owned_cleanup_lock || return 2
    verify_quarantine "$quarantine" "$quarantine_entry" || return 2
    mv -T -n "$quarantine" "$recovered_candidate" 2>/dev/null || return 2
    [ ! -e "$quarantine" ] && [ ! -L "$quarantine" ] || return 2
    if ! verify_candidate "$recovered_candidate" ||
      [ "$verified_candidate_name" != "$quarantine_entry" ]; then
      return 2
    fi
  done
  return 0
}

# Restore an interrupted capture only when its binary and sidecar still agree.
recover_orphaned_capture_quarantines() {
  [ "$hub_dir_available" = yes ] || return 0
  for quarantine in "$hub_dir"/.codexhub-capture-quarantine.*; do
    if [ ! -e "$quarantine" ] && [ ! -L "$quarantine" ]; then
      continue
    fi
    quarantine_name=${quarantine##*/}
    quarantine_suffix=${quarantine_name#".codexhub-capture-quarantine."}
    quarantine_nonce=${quarantine_suffix##*.}
    case "$quarantine_nonce" in "" | *[!0-9]*) return 2 ;; esac
    capture_name=${quarantine_suffix%".$quarantine_nonce"}
    [ "$quarantine_suffix" = "$capture_name.$quarantine_nonce" ] || return 2
    is_safe_capture_name "$capture_name" || return 2
    verify_capture_quarantine "$quarantine" "$capture_name" || return 2
    recovered_capture="$hub_dir/$capture_name"
    if [ -e "$recovered_capture" ] || [ -L "$recovered_capture" ]; then
      return 2
    fi
    verify_owned_cleanup_lock || return 2
    verify_capture_quarantine "$quarantine" "$capture_name" || return 2
    mv -T -n "$quarantine" "$recovered_capture" 2>/dev/null || return 2
    [ ! -e "$quarantine" ] && [ ! -L "$quarantine" ] || return 2
    if ! verify_capture_candidate "$recovered_capture" ||
      [ "$verified_capture_name" != "$capture_name" ]; then
      return 2
    fi
  done
  return 0
}

release_dir_for_binary() {
  binary=$1
  binary_real=$(readlink -f "$binary" 2>/dev/null) || return 1
  case "$binary_real" in
    "$release_root_real"/*/bin/codex | "$release_root_real"/*/codex)
      relative=${binary_real#"$release_root_real"/}
      release_entry=${relative%%/*}
      binary_relative_path=${relative#"$release_entry"/}
      case "$binary_relative_path" in bin/codex | codex) ;; *) return 1 ;; esac
      is_safe_release_entry "$release_entry" || return 1
      release_dir="$release_root_real/$release_entry"
      select_release_binary "$release_dir" "$release_entry" || return 1
      [ "$selected_release_binary_real" = "$binary_real" ] || return 1
      printf '%s\n' "$release_dir"
      return 0
      ;;
    *) return 1 ;;
  esac
}

refresh_protected_releases() {
  protected_current=""
  protected_target=""
  protected_capture=""
  protected_capture_path=""
  if [ -e "$current_link" ] || [ -L "$current_link" ]; then
    [ "$release_root_available" = yes ] || return 2
    [ -L "$current_link" ] || return 2
    current_real=$(readlink -f "$current_link" 2>/dev/null) || return 2
    case "$current_real" in
      "$release_root_real"/*)
        current_name=${current_real#"$release_root_real"/}
        case "$current_name" in */*) return 2 ;; esac
        is_safe_release_entry "$current_name" || return 2
        [ "$current_real" = "$release_root_real/$current_name" ] || return 2
        select_release_binary "$current_real" "$current_name" || return 2
        protected_current=$current_real
        ;;
      *) return 2 ;;
    esac
  fi
  if [ -e "$target_file" ] || [ -L "$target_file" ]; then
    [ -f "$target_file" ] && [ ! -L "$target_file" ] || return 2
    [ "$(wc -l <"$target_file" 2>/dev/null | tr -d '[:space:]')" = 1 ] || return 2
    target_value=$(sed -n '1p' "$target_file" 2>/dev/null) || return 2
    target_file_bytes=$(wc -c <"$target_file" 2>/dev/null | tr -d '[:space:]') || return 2
    target_value_bytes=$(printf '%s\n' "$target_value" | wc -c | tr -d '[:space:]') || return 2
    [ "$target_file_bytes" = "$target_value_bytes" ] || return 2
    case "$target_value" in /*) ;; *) return 2 ;; esac
    case "$target_value" in
      "$hub_dir"/codex-original.*)
        target_capture_name=${target_value##*/}
        is_safe_capture_name "$target_capture_name" || return 2
        verify_capture_candidate "$target_value" || return 2
        [ "$verified_capture_name" = "$target_capture_name" ] || return 2
        protected_capture=$verified_capture_real
        protected_capture_path=$target_value
        ;;
      "$hub_dir"/.codexhub-capture-quarantine.*) return 2 ;;
      *)
        target_release=""
        if [ "$release_root_available" = yes ]; then
          target_release=$(release_dir_for_binary "$target_value" 2>/dev/null || true)
        fi
        if [ -n "$target_release" ]; then
          protected_target=$target_release
        else
          case "$target_value" in
            "$release_root"/* | "$standalone_root/current"/*) return 2 ;;
          esac
        fi
        ;;
    esac
  fi
  return 0
}

read_cleanup_process_uid_starttime() {
  identity_proc_dir=$1
  observed_identity_uid=""
  observed_identity_starttime=""
  observed_identity_state=""
  [ -d "$identity_proc_dir" ] || return 1
  if [ ! -r "$identity_proc_dir/status" ] || [ ! -r "$identity_proc_dir/stat" ]; then
    [ -d "$identity_proc_dir" ] && return 2
    return 1
  fi
  identity_pair=$(awk '
    FILENAME == ARGV[1] {
      if ($1 == "Uid:") {
        uid_count += 1
        if (uid_count == 1) uid = $2
      }
      next
    }
    FILENAME == ARGV[2] {
      stat_count += 1
      if (stat_count == 1) {
        stat_line = $0
        sub(/^.*\) /, "", stat_line)
        field_count = split(stat_line, fields, /[[:space:]]+/)
        if (field_count >= 20) {
          state = fields[1]
          starttime = fields[20]
        }
      }
    }
    END {
      if (uid_count != 1 || stat_count != 1 ||
          uid !~ /^[0-9]+$/ || starttime !~ /^[0-9]+$/ ||
          state !~ /^[A-Za-z]$/) exit 2
      printf "%s\t%s\t%s\n", uid, starttime, state
    }
  ' "$identity_proc_dir/status" "$identity_proc_dir/stat" 2>/dev/null)
  identity_status=$?
  if [ "$identity_status" -ne 0 ]; then
    [ -d "$identity_proc_dir" ] && return 2
    return 1
  fi
  tab_character=$(printf '\t')
  identity_uid=${identity_pair%%"$tab_character"*}
  identity_remainder=${identity_pair#*"$tab_character"}
  [ "$identity_remainder" != "$identity_pair" ] || return 2
  identity_starttime=${identity_remainder%%"$tab_character"*}
  identity_state=${identity_remainder#*"$tab_character"}
  [ "$identity_state" != "$identity_remainder" ] || return 2
  case "$identity_uid:$identity_starttime" in *[!0-9:]* | :* | *: | *::* ) return 2 ;; esac
  case "$identity_state" in [A-Za-z]) ;; *) return 2 ;; esac
  observed_identity_uid=$identity_uid
  observed_identity_starttime=$identity_starttime
  observed_identity_state=$identity_state
  return 0
}

read_single_line_cleanup_field() {
  cleanup_field_path=$1
  observed_cleanup_field=""
  cleanup_field_count=0
  while IFS= read -r cleanup_field_line; do
    cleanup_field_count=$((cleanup_field_count + 1))
    [ "$cleanup_field_count" -eq 1 ] || return 2
    observed_cleanup_field=$cleanup_field_line
  done <"$cleanup_field_path"
  [ "$cleanup_field_count" -eq 1 ] || return 2
  return 0
}

read_current_uid_process_identity() {
  proc_dir=$1
  expected_uid=$2
  observed_proc_pid=""
  observed_proc_starttime=""
  observed_proc_exe=""
  [ -d "$proc_dir" ] || return 1
  proc_pid=${proc_dir##*/}
  case "$proc_pid" in "" | *[!0-9]*) return 2 ;; esac
  read_cleanup_process_uid_starttime "$proc_dir"
  proc_identity_status=$?
  case "$proc_identity_status" in
    0) ;;
    1) return 1 ;;
    *) return 2 ;;
  esac
  proc_uid=$observed_identity_uid
  proc_starttime=$observed_identity_starttime
  if [ "$proc_uid" != "$expected_uid" ]; then
    read_cleanup_process_uid_starttime "$proc_dir"
    non_user_identity_status=$?
    [ "$non_user_identity_status" -eq 0 ] || return 2
    proc_uid_after=$observed_identity_uid
    proc_starttime_after=$observed_identity_starttime
    if [ "$proc_uid_after" != "$proc_uid" ] || [ "$proc_starttime_after" != "$proc_starttime" ]; then
      return 2
    fi
    return 1
  fi
  proc_exe=$(readlink -f "$proc_dir/exe" 2>/dev/null || true)
  if [ -z "$proc_exe" ]; then
    [ -d "$proc_dir" ] && return 2
    return 1
  fi
  read_cleanup_process_uid_starttime "$proc_dir"
  proc_identity_after_status=$?
  [ "$proc_identity_after_status" -eq 0 ] || return 2
  proc_uid_after=$observed_identity_uid
  proc_starttime_after=$observed_identity_starttime
  if [ "$proc_uid_after" != "$proc_uid" ] || [ "$proc_starttime_after" != "$proc_starttime" ]; then
    return 2
  fi
  proc_exe_after=$(readlink -f "$proc_dir/exe" 2>/dev/null || true)
  [ -n "$proc_exe_after" ] || { [ -d "$proc_dir" ] && return 2; return 1; }
  [ "$proc_exe_after" = "$proc_exe" ] || return 2
  tab_character=$(printf '\t')
  newline_character='
'
  case "$proc_exe" in *"$tab_character"* | *"$newline_character"*) return 2 ;; esac
  observed_proc_pid=$proc_pid
  observed_proc_starttime=$proc_starttime
  observed_proc_exe=$proc_exe
  return 0
}

read_single_zombie_thread_count() {
  zombie_proc_dir=$1
  zombie_thread_count=$(awk '
    $1 == "Threads:" {
      count += 1
      if (count == 1) value = $2
    }
    END {
      if (count != 1 || value != "1") exit 2
      print value
    }
  ' "$zombie_proc_dir/status" 2>/dev/null)
  zombie_thread_status=$?
  if [ "$zombie_thread_status" -ne 0 ]; then
    [ -d "$zombie_proc_dir" ] && return 2
    return 1
  fi
  [ "$zombie_thread_count" = 1 ] || return 2
  return 0
}

# A zombie is harmless only when the kernel thread count and task set prove
# that no sibling remains alive with the old executable mapping.
verify_single_zombie_task() {
  zombie_proc_dir=$1
  zombie_pid=$2
  zombie_task_root="$zombie_proc_dir/task"
  if [ ! -d "$zombie_task_root" ] || [ -L "$zombie_task_root" ]; then
    [ -d "$zombie_proc_dir" ] && return 2
    return 1
  fi
  zombie_task_count=0
  for zombie_task_dir in "$zombie_task_root"/[0-9]*; do
    zombie_task_id=${zombie_task_dir##*/}
    if [ "$zombie_task_id" = '[0-9]*' ]; then
      [ ! -e "$zombie_task_dir" ] && [ ! -L "$zombie_task_dir" ] || return 2
      continue
    fi
    case "$zombie_task_id" in "" | *[!0-9]*) return 2 ;; esac
    # A numeric TID captured by glob expansion but gone before inspection is
    # a task-list race, not an ignorable no-match entry.
    [ -d "$zombie_task_dir" ] && [ ! -L "$zombie_task_dir" ] || return 2
    zombie_task_count=$((zombie_task_count + 1))
    [ "$zombie_task_id" = "$zombie_pid" ] || return 2
  done
  [ "$zombie_task_count" -eq 1 ] || return 2
  return 0
}

# Staged Update backups are reversible, so unreadable executables may be
# ignored only for stable, strictly classified session infrastructure or a
# kernel-confirmed single-thread zombie. Managed-only cleanup stays strict.
read_staged_session_process_identity() {
  session_proc_dir=$1
  expected_uid=$2
  observed_session_pid=""
  observed_session_starttime=""
  observed_session_state=""
  observed_session_comm_hex=""
  observed_session_cmdline_hex=""
  observed_session_kind=""
  [ "$cleanup_policy" = verified-older-than ] || return 2
  [ "$cleanup_backup_mode" = staged ] || return 2
  [ -d "$session_proc_dir" ] || return 1
  session_pid=${session_proc_dir##*/}
  case "$session_pid" in "" | *[!0-9]*) return 2 ;; esac
  for session_field in status stat comm cmdline; do
    if [ ! -r "$session_proc_dir/$session_field" ]; then
      [ -d "$session_proc_dir" ] && return 2
      return 1
    fi
  done

  read_cleanup_process_uid_starttime "$session_proc_dir"
  session_identity_status=$?
  case "$session_identity_status" in
    0) ;;
    1) return 1 ;;
    *) return 2 ;;
  esac
  session_uid=$observed_identity_uid
  session_starttime=$observed_identity_starttime
  session_state=$observed_identity_state
  [ "$session_uid" = "$expected_uid" ] || return 2
  read_single_line_cleanup_field "$session_proc_dir/comm" || return 2
  session_comm=$observed_cleanup_field
  session_comm_hex=$(printf '%s' "$session_comm" | od -An -v -tx1 | tr -d '[:space:]') || return 2
  case "$session_comm_hex" in "" | *[!0-9a-fA-F]*) return 2 ;; esac
  session_cmdline_hex=$(od -An -v -tx1 "$session_proc_dir/cmdline" 2>/dev/null | tr -d '[:space:]') || return 2
  case "$session_cmdline_hex" in *[!0-9a-fA-F]*) return 2 ;; esac
  session_exe=$(readlink -f "$session_proc_dir/exe" 2>/dev/null || true)
  [ -z "$session_exe" ] || return 2

  session_kind=""
  if [ "$session_state" = Z ]; then
    [ -z "$session_cmdline_hex" ] || return 2
    read_single_zombie_thread_count "$session_proc_dir" || return 2
    verify_single_zombie_task "$session_proc_dir" "$session_pid" || return 2
    session_state_class=zombie
    session_cmdline_identity=empty
    session_kind=zombie
  else
    case "$session_cmdline_hex" in "" | *00) ;; *) return 2 ;; esac
    [ -n "$session_cmdline_hex" ] || return 2
    [ "${#session_cmdline_hex}" -le 8192 ] || return 2
    session_arg0_hex=${session_cmdline_hex%%00*}
    [ -n "$session_arg0_hex" ] || return 2
    [ "${#session_arg0_hex}" -le 1024 ] || return 2
    case "$session_comm" in
      sshd)
        case "$session_arg0_hex" in
          737368643a20??*) session_kind=sshd-session ;;
          *) return 2 ;;
        esac
        ;;
      "(sd-pam)")
        [ "$session_arg0_hex" = 2873642d70616d29 ] || return 2
        session_kind=sd-pam
        ;;
      sftp-server)
        case "$session_arg0_hex" in
          736674702d736572766572 | *2f736674702d736572766572) session_kind=sftp-server ;;
          *) return 2 ;;
        esac
        ;;
      fusermount3)
        case "$session_arg0_hex" in
          66757365726d6f756e7433 | *2f66757365726d6f756e7433) session_kind=fusermount3 ;;
          *) return 2 ;;
        esac
        ;;
      systemd)
        case "$session_cmdline_hex" in
          2f7573722f6c69622f73797374656d642f73797374656d64002d2d7573657200|2f6c69622f73797374656d642f73797374656d64002d2d7573657200)
            session_kind=systemd-user
            ;;
          *) return 2 ;;
        esac
        ;;
      *) return 2 ;;
    esac
    session_state_class=live
    session_cmdline_identity=$session_cmdline_hex
  fi

  read_cleanup_process_uid_starttime "$session_proc_dir"
  session_identity_after_status=$?
  [ "$session_identity_after_status" -eq 0 ] || return 2
  session_uid_after=$observed_identity_uid
  session_starttime_after=$observed_identity_starttime
  session_state_after=$observed_identity_state
  read_single_line_cleanup_field "$session_proc_dir/comm" || return 2
  session_comm_after=$observed_cleanup_field
  session_comm_hex_after=$(printf '%s' "$session_comm_after" | od -An -v -tx1 | tr -d '[:space:]') || return 2
  case "$session_comm_hex_after" in "" | *[!0-9a-fA-F]*) return 2 ;; esac
  session_cmdline_hex_after=$(od -An -v -tx1 "$session_proc_dir/cmdline" 2>/dev/null | tr -d '[:space:]') || return 2
  case "$session_cmdline_hex_after" in *[!0-9a-fA-F]*) return 2 ;; esac
  session_exe_after=$(readlink -f "$session_proc_dir/exe" 2>/dev/null || true)
  [ "$session_uid_after" = "$session_uid" ] || return 2
  [ "$session_starttime_after" = "$session_starttime" ] || return 2
  [ "$session_comm_hex_after" = "$session_comm_hex" ] || return 2
  [ -z "$session_exe_after" ] || return 2
  if [ "$session_state_class" = zombie ]; then
    [ "$session_state_after" = Z ] || return 2
    [ -z "$session_cmdline_hex_after" ] || return 2
    read_single_zombie_thread_count "$session_proc_dir" || return 2
    verify_single_zombie_task "$session_proc_dir" "$session_pid" || return 2
    session_cmdline_identity_after=empty
  else
    [ "$session_state_after" != Z ] || return 2
    case "$session_cmdline_hex_after" in "" | *00) ;; *) return 2 ;; esac
    session_cmdline_identity_after=$session_cmdline_hex_after
  fi
  [ "$session_cmdline_identity_after" = "$session_cmdline_identity" ] || return 2

  observed_session_pid=$session_pid
  observed_session_starttime=$session_starttime
  observed_session_state=$session_state_class
  observed_session_comm_hex=$session_comm_hex
  observed_session_cmdline_hex=$session_cmdline_identity
  observed_session_kind=$session_kind
  return 0
}

staged_session_process_matches_snapshot() {
  session_proc_dir=$1
  expected_uid=$2
  read_staged_session_process_identity "$session_proc_dir" "$expected_uid"
  session_identity_status=$?
  [ "$session_identity_status" -eq 0 ] || return "$session_identity_status"
  tab_character=$(printf '\t')
  while IFS="$tab_character" read -r session_pid session_starttime session_state session_comm_hex session_cmdline_hex session_kind; do
    if [ "$session_pid" = "$observed_session_pid" ] &&
      [ "$session_starttime" = "$observed_session_starttime" ] &&
      [ "$session_state" = "$observed_session_state" ] &&
      [ "$session_comm_hex" = "$observed_session_comm_hex" ] &&
      [ "$session_cmdline_hex" = "$observed_session_cmdline_hex" ] &&
      [ "$session_kind" = "$observed_session_kind" ]; then
      return 0
    fi
  done <"$ignored_session_processes"
  return 2
}

staged_session_pid_was_snapshotted() {
  session_proc_dir=$1
  session_pid=${session_proc_dir##*/}
  case "$session_pid" in "" | *[!0-9]*) return 2 ;; esac
  tab_character=$(printf '\t')
  while IFS="$tab_character" read -r snap_pid snap_starttime snap_state snap_comm_hex snap_cmdline_hex snap_kind; do
    [ "$snap_pid" = "$session_pid" ] && return 0
  done <"$ignored_session_processes"
  return 1
}

scan_current_uid_executables() {
  : >"$proc_exes" || return 2
  : >"$ignored_session_processes" || return 2
  ignored_session_process_count=0
  current_uid=$(id -u 2>/dev/null) || return 2
  for proc_dir in "$proc_root"/[0-9]*; do
    [ -d "$proc_dir" ] || continue
    read_current_uid_process_identity "$proc_dir" "$current_uid"
    identity_status=$?
    case "$identity_status" in
      0) printf '%s\t%s\t%s\n' "$observed_proc_pid" "$observed_proc_starttime" "$observed_proc_exe" >>"$proc_exes" || return 2 ;;
      1) ;;
      *)
        read_staged_session_process_identity "$proc_dir" "$current_uid"
        session_identity_status=$?
        case "$session_identity_status" in
          0)
            printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$observed_session_pid" "$observed_session_starttime" \
              "$observed_session_state" "$observed_session_comm_hex" "$observed_session_cmdline_hex" \
              "$observed_session_kind" >>"$ignored_session_processes" || return 2
            ignored_session_process_count=$((ignored_session_process_count + 1))
            ;;
          1) ;;
          *) return 2 ;;
        esac
        ;;
    esac
  done
  return 0
}

release_in_snapshot() {
  release_dir=$1
  tab_character=$(printf '\t')
  while IFS="$tab_character" read -r proc_pid proc_starttime proc_exe; do
    case "$proc_pid:$proc_starttime" in *[!0-9:]* | :* | *: | *::* ) return 0 ;; esac
    case "$proc_exe" in "$release_dir"/*) return 0 ;; esac
  done <"$proc_exes"
  return 1
}

capture_in_snapshot() {
  capture_real=$1
  tab_character=$(printf '\t')
  while IFS="$tab_character" read -r proc_pid proc_starttime proc_exe; do
    case "$proc_pid:$proc_starttime" in *[!0-9:]* | :* | *: | *::* ) return 0 ;; esac
    [ "$proc_exe" = "$capture_real" ] && return 0
  done <"$proc_exes"
  return 1
}

release_in_use_now() {
  release_dir=$1
  second_release_dir=${2:-}
  current_uid=$(id -u 2>/dev/null) || return 2
  for proc_dir in "$proc_root"/[0-9]*; do
    [ -d "$proc_dir" ] || continue
    staged_session_pid_was_snapshotted "$proc_dir"
    session_snapshot_pid_status=$?
    case "$session_snapshot_pid_status" in
      0)
        staged_session_process_matches_snapshot "$proc_dir" "$current_uid"
        session_snapshot_status=$?
        case "$session_snapshot_status" in
          0 | 1) continue ;;
          *) return 2 ;;
        esac
        ;;
      1) ;;
      *) return 2 ;;
    esac
    read_current_uid_process_identity "$proc_dir" "$current_uid"
    identity_status=$?
    case "$identity_status" in
      0)
        case "$observed_proc_exe" in "$release_dir"/*) return 0 ;; esac
        if [ -n "$second_release_dir" ]; then
          case "$observed_proc_exe" in "$second_release_dir"/*) return 0 ;; esac
        fi
        ;;
      1) ;;
      *)
        staged_session_process_matches_snapshot "$proc_dir" "$current_uid"
        session_snapshot_status=$?
        case "$session_snapshot_status" in
          0 | 1) ;;
          *) return 2 ;;
        esac
        ;;
    esac
  done
  return 1
}

capture_in_use_now() {
  capture_real=$1
  second_capture_real=${2:-}
  current_uid=$(id -u 2>/dev/null) || return 2
  for proc_dir in "$proc_root"/[0-9]*; do
    [ -d "$proc_dir" ] || continue
    staged_session_pid_was_snapshotted "$proc_dir"
    session_snapshot_pid_status=$?
    case "$session_snapshot_pid_status" in
      0)
        staged_session_process_matches_snapshot "$proc_dir" "$current_uid"
        session_snapshot_status=$?
        case "$session_snapshot_status" in
          0 | 1) continue ;;
          *) return 2 ;;
        esac
        ;;
      1) ;;
      *) return 2 ;;
    esac
    read_current_uid_process_identity "$proc_dir" "$current_uid"
    identity_status=$?
    case "$identity_status" in
      0)
        if [ "$observed_proc_exe" = "$capture_real" ] ||
          { [ -n "$second_capture_real" ] && [ "$observed_proc_exe" = "$second_capture_real" ]; }; then
          return 0
        fi
        ;;
      1) ;;
      *)
        staged_session_process_matches_snapshot "$proc_dir" "$current_uid"
        session_snapshot_status=$?
        case "$session_snapshot_status" in
          0 | 1) ;;
          *) return 2 ;;
        esac
        ;;
    esac
  done
  return 1
}

defer_all_direct_entries() {
  reason=$1
  if [ "$release_root_available" = yes ]; then
    for candidate in "$release_root"/* "$release_root"/.codexhub-quarantine.*; do
      if [ ! -e "$candidate" ] && [ ! -L "$candidate" ]; then
        continue
      fi
      scanned=$((scanned + 1))
      deferred=$((deferred + 1))
    done
  fi
  if [ "$hub_dir_available" = yes ]; then
    for candidate in "$hub_dir"/codex-original.* "$hub_dir"/.codexhub-capture-quarantine.*; do
      if [ ! -e "$candidate" ] && [ ! -L "$candidate" ]; then
        continue
      fi
      candidate_name=${candidate##*/}
      case "$candidate_name" in
        *"$capture_marker_suffix" | *"$capture_marker_suffix".tmp.*) continue ;;
      esac
      scanned=$((scanned + 1))
      deferred=$((deferred + 1))
    done
  fi
  if [ "$scanned" -eq 0 ]; then
    emit_cleanup_result not-applicable no-release-directories
  else
    emit_cleanup_result deferred "$reason"
  fi
  exit 0
}

defer_active_candidate() {
  deferred=$((deferred + 1))
  cleanup_reason=$1
  if ! restore_active_quarantine; then
    cleanup_reason=quarantine-restore-failed
    fail_cleanup quarantine-restore-failed
  fi
}

case "$cleanup_policy" in
  managed-only)
    [ -z "$cleanup_verified_version" ] || fail_cleanup cleanup-policy-invalid
    [ "$cleanup_backup_mode" = none ] || fail_cleanup cleanup-policy-invalid
    ;;
  verified-older-than)
    [ -n "$cleanup_verified_version" ] || fail_cleanup cleanup-policy-invalid
    [ "$cleanup_backup_mode" = staged ] || fail_cleanup cleanup-policy-invalid
    version_is_strictly_lower "$cleanup_verified_version" "$cleanup_verified_version"
    cleanup_version_status=$?
    [ "$cleanup_version_status" -eq 1 ] || fail_cleanup cleanup-policy-version-invalid
    ;;
  *) fail_cleanup cleanup-policy-invalid ;;
esac

if [ ! -d "$proc_root" ]; then
  defer_all_direct_entries proc-unavailable
fi
acquire_cleanup_lock
cleanup_lock_status=$?
case "$cleanup_lock_status" in
  0) ;;
  3) defer_all_direct_entries cleanup-lock-active ;;
  2) defer_all_direct_entries cleanup-lock-identity-unknown ;;
  *) fail_cleanup cleanup-lock-acquire-failed ;;
esac
recover_orphaned_quarantines || defer_all_direct_entries quarantine-recovery-unsafe
recover_orphaned_capture_quarantines || defer_all_direct_entries capture-quarantine-recovery-unsafe
refresh_protected_releases || defer_all_direct_entries protected-identity-unknown
scan_current_uid_executables || defer_all_direct_entries proc-identity-unknown

if [ "$release_root_available" = yes ]; then
for candidate in "$release_root"/*; do
  if [ ! -e "$candidate" ] && [ ! -L "$candidate" ]; then
    continue
  fi
  scanned=$((scanned + 1))
  if ! verify_release_identity "$candidate"; then
    retained=$((retained + 1))
    continue
  fi
  candidate_real=$verified_candidate_real
  candidate_name=$verified_candidate_name
  candidate_version=$verified_candidate_version
  if [ "$cleanup_policy" = verified-older-than ]; then
    version_is_strictly_lower "$candidate_version" "$cleanup_verified_version"
    version_relation=$?
    case "$version_relation" in
      0) ;;
      1)
        retained=$((retained + 1))
        continue
        ;;
      *)
        deferred=$((deferred + 1))
        cleanup_reason=release-version-incomparable
        continue
        ;;
    esac
  fi
  if [ "$candidate_real" = "$protected_current" ] || [ "$candidate_real" = "$protected_target" ]; then
    retained=$((retained + 1))
    continue
  fi
  if ! managed_marker_valid "$candidate" "$candidate_version"; then
    marker="$candidate/$marker_name"
    if [ "$cleanup_policy" != verified-older-than ] ||
      [ -e "$marker" ] || [ -L "$marker" ]; then
      retained=$((retained + 1))
      continue
    fi
    adopt_verified_release "$candidate" "$candidate_real" "$candidate_name" "$candidate_version"
    adoption_status=$?
    case "$adoption_status" in
      0) ;;
      1)
        retained=$((retained + 1))
        continue
        ;;
      *)
        deferred=$((deferred + 1))
        cleanup_reason=release-adoption-raced
        continue
        ;;
    esac
  fi
  if release_in_snapshot "$candidate_real"; then
    retained=$((retained + 1))
    continue
  fi

  # Re-read every mutable identity before atomically hiding the old launch path.
  if ! verify_candidate "$candidate" ||
    [ "$verified_candidate_real" != "$candidate_real" ] ||
    [ "$verified_candidate_name" != "$candidate_name" ] ||
    [ "$verified_candidate_version" != "$candidate_version" ]; then
    deferred=$((deferred + 1))
    cleanup_reason=candidate-raced
    continue
  fi
  if ! refresh_protected_releases; then
    deferred=$((deferred + 1))
    cleanup_reason=protected-identity-unknown
    continue
  fi
  if [ "$candidate_real" = "$protected_current" ] || [ "$candidate_real" = "$protected_target" ]; then
    retained=$((retained + 1))
    continue
  fi
  release_in_use_now "$candidate_real"
  in_use_status=$?
  case "$in_use_status" in
    0)
      retained=$((retained + 1))
      continue
      ;;
    1) ;;
    *)
      deferred=$((deferred + 1))
      cleanup_reason=proc-identity-unknown
      continue
      ;;
  esac

  quarantine="$release_root/.codexhub-quarantine.$candidate_name.$$"
  if [ -e "$quarantine" ] || [ -L "$quarantine" ]; then
    deferred=$((deferred + 1))
    cleanup_reason=quarantine-path-collision
    continue
  fi
  if ! verify_owned_cleanup_lock ||
    ! verify_candidate "$candidate" ||
    [ "$verified_candidate_real" != "$candidate_real" ] ||
    [ "$verified_candidate_name" != "$candidate_name" ] ||
    [ "$verified_candidate_version" != "$candidate_version" ]; then
    deferred=$((deferred + 1))
    cleanup_reason=cleanup-lock-or-candidate-raced
    continue
  fi
  if ! mv -T -n "$candidate" "$quarantine" ||
    { [ -e "$candidate" ] || [ -L "$candidate" ]; }; then
    deferred=$((deferred + 1))
    cleanup_reason=quarantine-stage-failed
    continue
  fi
  active_candidate=$candidate
  active_quarantine=$quarantine
  active_kind=release

  # Once isolated, ordinary launch paths can no longer start this release.
  # Re-check identity, protection, and /proc before deleting the quarantine.
  if ! verify_quarantine "$quarantine" "$candidate_name" ||
    [ "$verified_quarantine_version" != "$candidate_version" ] ||
    [ -e "$candidate" ] || [ -L "$candidate" ]; then
    defer_active_candidate quarantine-identity-unknown
    continue
  fi
  isolated_real=$verified_quarantine_real
  if ! refresh_protected_releases; then
    defer_active_candidate protected-identity-unknown
    continue
  fi
  # One process snapshot checks both the former and isolated executable paths.
  release_in_use_now "$candidate_real" "$isolated_real"
  quarantined_in_use_status=$?
  case "$quarantined_in_use_status" in
    0)
      defer_active_candidate candidate-became-active
      continue
      ;;
    1) ;;
    *)
      defer_active_candidate proc-identity-unknown
      continue
      ;;
  esac
  if ! verify_quarantine "$quarantine" "$candidate_name" ||
    [ "$verified_quarantine_real" != "$isolated_real" ] ||
    [ "$verified_quarantine_version" != "$candidate_version" ] ||
    [ -e "$candidate" ] || [ -L "$candidate" ]; then
    defer_active_candidate quarantine-raced
    continue
  fi
  if ! refresh_protected_releases; then
    defer_active_candidate protected-identity-unknown
    continue
  fi
  release_in_use_now "$isolated_real"
  isolated_in_use_status=$?
  case "$isolated_in_use_status" in
    0)
      defer_active_candidate candidate-became-active
      continue
      ;;
    1) ;;
    *)
      defer_active_candidate proc-identity-unknown
      continue
      ;;
  esac
  if ! verify_owned_cleanup_lock ||
    ! verify_quarantine "$quarantine" "$candidate_name" ||
    [ "$verified_quarantine_real" != "$isolated_real" ] ||
    [ "$verified_quarantine_version" != "$candidate_version" ] ||
    [ -e "$candidate" ] || [ -L "$candidate" ]; then
    defer_active_candidate cleanup-lock-or-quarantine-raced
    continue
  fi
  if [ "$cleanup_backup_mode" = staged ]; then
    if ! ensure_update_backup; then
      defer_active_candidate update-backup-unavailable
      continue
    fi
    backup_destination="$backup_releases/$candidate_name"
    if [ -e "$backup_destination" ] || [ -L "$backup_destination" ]; then
      defer_active_candidate update-backup-collision
      continue
    fi
    isolated_identity=$(stat -c '%d:%i' "$quarantine" 2>/dev/null) || {
      defer_active_candidate quarantine-identity-unknown
      continue
    }
    if ! mv -T -n "$quarantine" "$backup_destination" 2>/dev/null ||
      [ -e "$quarantine" ] || [ -L "$quarantine" ]; then
      defer_active_candidate update-backup-stage-failed
      continue
    fi
    if [ -e "$candidate" ] || [ -L "$candidate" ]; then
      active_candidate=""
      active_quarantine=""
      active_kind=""
      deferred=$((deferred + 1))
      fail_cleanup update-backup-original-raced
    fi
    if ! verify_backup_release "$backup_destination" "$candidate_name" ||
      [ "$verified_backup_release_version" != "$candidate_version" ] ||
      [ "$(stat -c '%d:%i' "$backup_destination" 2>/dev/null)" != "$isolated_identity" ] ||
      [ -e "$candidate" ] || [ -L "$candidate" ]; then
      active_candidate=""
      active_quarantine=""
      active_kind=""
      deferred=$((deferred + 1))
      fail_cleanup update-backup-verification-failed
    fi
    if ! backup_residual_links_for_release "$candidate" "$quarantine" "$candidate_name"; then
      deferred=$((deferred + 1))
      fail_cleanup residual-link-backup-failed
    fi
    # Close the zero-link and post-loop recreation window before completion.
    if [ -e "$candidate" ] || [ -L "$candidate" ]; then
      active_candidate=""
      active_quarantine=""
      active_kind=""
      deferred=$((deferred + 1))
      fail_cleanup update-backup-original-raced
    fi
    if ! printf 'release=%s|version=%s\n' "$candidate_name" "$candidate_version" >>"$backup_manifest"; then
      deferred=$((deferred + 1))
      fail_cleanup update-backup-manifest-failed
    fi
    active_candidate=""
    active_quarantine=""
    active_kind=""
    backed_up=$((backed_up + 1))
  else
    # Managed-only cleanup keeps the existing destructive semantics.
    active_candidate=""
    active_quarantine=""
    active_kind=""
    if ! rm -rf "$quarantine"; then
      deferred=$((deferred + 1))
      fail_cleanup release-removal-failed
    fi
    if [ -e "$quarantine" ] || [ -L "$quarantine" ]; then
      deferred=$((deferred + 1))
      fail_cleanup release-removal-unconfirmed
    fi
  fi
  removed=$((removed + 1))
done
fi

if [ "$hub_dir_available" = yes ]; then
for candidate in "$hub_dir"/codex-original.*; do
  if [ ! -e "$candidate" ] && [ ! -L "$candidate" ]; then
    continue
  fi
  candidate_name=${candidate##*/}
  # Sidecars describe the binary candidate and are never counted separately.
  case "$candidate_name" in
    *"$capture_marker_suffix" | *"$capture_marker_suffix".tmp.*) continue ;;
  esac
  scanned=$((scanned + 1))
  if ! is_safe_capture_name "$candidate_name" || ! verify_capture_candidate "$candidate"; then
    retained=$((retained + 1))
    continue
  fi
  candidate_real=$verified_capture_real
  candidate_version=$verified_capture_version
  if { [ "$candidate" = "$protected_capture_path" ] && [ "$candidate_real" = "$protected_capture" ]; }; then
    retained=$((retained + 1))
    continue
  fi
  if capture_in_snapshot "$candidate_real"; then
    retained=$((retained + 1))
    continue
  fi

  if ! verify_capture_candidate "$candidate" ||
    [ "$verified_capture_real" != "$candidate_real" ] ||
    [ "$verified_capture_name" != "$candidate_name" ] ||
    [ "$verified_capture_version" != "$candidate_version" ]; then
    deferred=$((deferred + 1))
    cleanup_reason=capture-candidate-raced
    continue
  fi
  if ! refresh_protected_releases; then
    deferred=$((deferred + 1))
    cleanup_reason=protected-identity-unknown
    continue
  fi
  if { [ "$candidate" = "$protected_capture_path" ] && [ "$candidate_real" = "$protected_capture" ]; }; then
    retained=$((retained + 1))
    continue
  fi
  capture_in_use_now "$candidate_real"
  in_use_status=$?
  case "$in_use_status" in
    0)
      retained=$((retained + 1))
      continue
      ;;
    1) ;;
    *)
      deferred=$((deferred + 1))
      cleanup_reason=proc-identity-unknown
      continue
      ;;
  esac

  quarantine="$hub_dir/.codexhub-capture-quarantine.$candidate_name.$$"
  if [ -e "$quarantine" ] || [ -L "$quarantine" ]; then
    deferred=$((deferred + 1))
    cleanup_reason=capture-quarantine-path-collision
    continue
  fi
  if ! verify_owned_cleanup_lock ||
    ! verify_capture_candidate "$candidate" ||
    [ "$verified_capture_real" != "$candidate_real" ] ||
    [ "$verified_capture_name" != "$candidate_name" ] ||
    [ "$verified_capture_version" != "$candidate_version" ]; then
    deferred=$((deferred + 1))
    cleanup_reason=cleanup-lock-or-capture-raced
    continue
  fi
  if ! mv -T -n "$candidate" "$quarantine" ||
    { [ -e "$candidate" ] || [ -L "$candidate" ]; }; then
    deferred=$((deferred + 1))
    cleanup_reason=capture-quarantine-stage-failed
    continue
  fi
  active_candidate=$candidate
  active_quarantine=$quarantine
  active_kind=capture

  # The original launch path is hidden; now repeat every mutable safety check.
  if ! verify_capture_quarantine "$quarantine" "$candidate_name" ||
    [ "$verified_capture_quarantine_version" != "$candidate_version" ] ||
    [ -e "$candidate" ] || [ -L "$candidate" ]; then
    defer_active_candidate capture-quarantine-identity-unknown
    continue
  fi
  isolated_real=$verified_capture_quarantine_real
  if ! refresh_protected_releases; then
    defer_active_candidate protected-identity-unknown
    continue
  fi
  # One process snapshot checks both the former and isolated executable paths.
  capture_in_use_now "$candidate_real" "$isolated_real"
  quarantined_in_use_status=$?
  case "$quarantined_in_use_status" in
    0)
      defer_active_candidate capture-became-active
      continue
      ;;
    1) ;;
    *)
      defer_active_candidate proc-identity-unknown
      continue
      ;;
  esac
  if ! verify_capture_quarantine "$quarantine" "$candidate_name" ||
    [ "$verified_capture_quarantine_real" != "$isolated_real" ] ||
    [ "$verified_capture_quarantine_version" != "$candidate_version" ] ||
    [ -e "$candidate" ] || [ -L "$candidate" ]; then
    defer_active_candidate capture-quarantine-raced
    continue
  fi
  if ! refresh_protected_releases; then
    defer_active_candidate protected-identity-unknown
    continue
  fi
  capture_in_use_now "$isolated_real"
  isolated_in_use_status=$?
  case "$isolated_in_use_status" in
    0)
      defer_active_candidate capture-became-active
      continue
      ;;
    1) ;;
    *)
      defer_active_candidate proc-identity-unknown
      continue
      ;;
  esac

  # After destructive removal begins, a partial or unknown file is never restored.
  capture_marker="$hub_dir/$candidate_name$capture_marker_suffix"
  if ! verify_owned_cleanup_lock ||
    ! verify_capture_quarantine "$quarantine" "$candidate_name" ||
    [ "$verified_capture_quarantine_real" != "$isolated_real" ] ||
    [ "$verified_capture_quarantine_version" != "$candidate_version" ] ||
    [ -e "$candidate" ] || [ -L "$candidate" ]; then
    defer_active_candidate cleanup-lock-or-capture-quarantine-raced
    continue
  fi
  if [ "$cleanup_backup_mode" = staged ]; then
    if ! ensure_update_backup; then
      defer_active_candidate update-backup-unavailable
      continue
    fi
    backup_capture="$backup_captures/$candidate_name"
    backup_capture_marker="$backup_capture$capture_marker_suffix"
    if [ -e "$backup_capture" ] || [ -L "$backup_capture" ] ||
      [ -e "$backup_capture_marker" ] || [ -L "$backup_capture_marker" ]; then
      defer_active_candidate update-backup-collision
      continue
    fi
    if ! verify_owned_cleanup_lock || ! capture_marker_valid "$candidate_name" "$candidate_version"; then
      defer_active_candidate capture-marker-identity-changed
      continue
    fi
    if ! ln "$capture_marker" "$backup_capture_marker" 2>/dev/null; then
      defer_active_candidate capture-marker-backup-failed
      continue
    fi
    capture_marker_identity=$(stat -c '%d:%i' "$capture_marker" 2>/dev/null) || {
      deferred=$((deferred + 1))
      fail_cleanup capture-marker-backup-unverified
    }
    if [ "$capture_marker_identity" != \
      "$(stat -c '%d:%i' "$backup_capture_marker" 2>/dev/null)" ]; then
      deferred=$((deferred + 1))
      fail_cleanup capture-marker-backup-unverified
    fi
    isolated_identity=$(stat -c '%d:%i' "$quarantine" 2>/dev/null) || {
      deferred=$((deferred + 1))
      fail_cleanup capture-quarantine-identity-unknown
    }
    if ! mv -T -n "$quarantine" "$backup_capture" 2>/dev/null ||
      [ -e "$quarantine" ] || [ -L "$quarantine" ]; then
      defer_active_candidate capture-backup-stage-failed
      continue
    fi
    if ! verify_backup_capture "$backup_capture" "$candidate_name" "$candidate_version" ||
      [ "$(stat -c '%d:%i' "$backup_capture" 2>/dev/null)" != "$isolated_identity" ] ||
      [ -e "$candidate" ] || [ -L "$candidate" ]; then
      active_candidate=""
      active_quarantine=""
      active_kind=""
      deferred=$((deferred + 1))
      fail_cleanup capture-backup-verification-failed
    fi
    # Bind the final unlink to the marker that was hard-linked into the backup.
    if ! verify_owned_cleanup_lock ||
      [ -e "$candidate" ] || [ -L "$candidate" ] ||
      [ ! -f "$capture_marker" ] || [ -L "$capture_marker" ] ||
      [ "$(stat -c '%d:%i' "$capture_marker" 2>/dev/null)" != "$capture_marker_identity" ] ||
      [ "$(stat -c '%d:%i' "$backup_capture_marker" 2>/dev/null)" != "$capture_marker_identity" ]; then
      active_candidate=""
      active_quarantine=""
      active_kind=""
      deferred=$((deferred + 1))
      fail_cleanup capture-marker-identity-changed
    fi
    active_candidate=""
    active_quarantine=""
    active_kind=""
    if ! rm -f "$capture_marker" || [ -e "$capture_marker" ] || [ -L "$capture_marker" ]; then
      deferred=$((deferred + 1))
      fail_cleanup capture-marker-removal-failed
    fi
    if ! printf 'capture=%s|version=%s\n' "$candidate_name" "$candidate_version" >>"$backup_manifest"; then
      deferred=$((deferred + 1))
      fail_cleanup update-backup-manifest-failed
    fi
    backed_up=$((backed_up + 1))
  else
    active_candidate=""
    active_quarantine=""
    active_kind=""
    if ! rm -f "$quarantine"; then
      deferred=$((deferred + 1))
      fail_cleanup capture-removal-failed
    fi
    if [ -e "$quarantine" ] || [ -L "$quarantine" ]; then
      deferred=$((deferred + 1))
      fail_cleanup capture-removal-unconfirmed
    fi
    if ! verify_owned_cleanup_lock || ! capture_marker_valid "$candidate_name" "$candidate_version"; then
      fail_cleanup capture-marker-identity-changed
    fi
    if ! rm -f "$capture_marker"; then
      fail_cleanup capture-marker-removal-failed
    fi
    if [ -e "$capture_marker" ] || [ -L "$capture_marker" ]; then
      fail_cleanup capture-marker-removal-unconfirmed
    fi
  fi
  removed=$((removed + 1))
done
fi

if [ "$scanned" -eq 0 ]; then
  emit_cleanup_result not-applicable no-release-directories
elif [ "$deferred" -gt 0 ]; then
  emit_cleanup_result deferred "$cleanup_reason"
else
  emit_cleanup_result completed cleanup-complete
fi
exit 0
"###;

pub(crate) fn remote_codex_release_cleanup_script() -> &'static str {
    REMOTE_CODEX_RELEASE_CLEANUP_SCRIPT
}

pub(crate) fn remote_codex_release_cleanup_script_with_policy(
    policy: &CodexReleaseCleanupPolicy,
) -> Result<String, String> {
    match policy {
        CodexReleaseCleanupPolicy::ManagedOnly => Ok(format!(
            "codexhub_cleanup_policy='managed-only'\ncodexhub_cleanup_verified_version=''\ncodexhub_cleanup_backup_mode='none'\n{}",
            remote_codex_release_cleanup_script()
        )),
        CodexReleaseCleanupPolicy::VerifiedOlderThan(version) => {
            let version = normalized_codex_version(version).ok_or_else(|| {
                "The verified post-update Codex version could not be normalized safely.".to_string()
            })?;
            Ok(format!(
                "codexhub_cleanup_policy='verified-older-than'\ncodexhub_cleanup_verified_version='{version}'\ncodexhub_cleanup_backup_mode='staged'\n{}",
                remote_codex_release_cleanup_script()
            ))
        }
    }
}

pub(crate) fn cleanup_remote_codex_releases(
    alias: &str,
    timeout: u64,
    policy: CodexReleaseCleanupPolicy,
) -> Result<CodexReleaseCleanupResult, String> {
    let script = remote_codex_release_cleanup_script_with_policy(&policy)?;
    let cleanup_timeout = timeout.max(REMOTE_CODEX_RELEASE_CLEANUP_TIMEOUT_MS);
    let output = ssh::run_ssh_script_with_extended_timeout(alias, &script, cleanup_timeout)
        .map_err(|_| "Could not run managed Codex runtime cleanup.".to_string())?;
    parse_remote_codex_release_cleanup_output(&output)
}

pub(crate) fn probe_remote_strict_current_version(
    alias: &str,
    timeout: u64,
) -> Result<Option<StrictCurrentRuntime>, String> {
    let output = ssh::run_ssh_script(alias, REMOTE_STRICT_CURRENT_VERSION_SCRIPT, timeout)
        .map_err(|_| "Could not inspect standalone/current before maintenance.".to_string())?;
    parse_remote_strict_current_version_output(&output)
}

pub(crate) fn remote_codex_runtime_reconcile_script() -> &'static str {
    static SCRIPT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SCRIPT
        .get_or_init(|| {
            format!(
                "codexhub_runtime_allow_unverified_state=yes\n{REMOTE_CODEX_RUNTIME_WRITER_LOCK_PRELUDE}\n{REMOTE_CODEX_RUNTIME_RECONCILE_SCRIPT}"
            )
        })
        .as_str()
}

#[cfg(test)]
pub(crate) fn remote_codex_runtime_reconcile_script_with_minimum(
    minimum_version: Option<&str>,
) -> Result<String, String> {
    remote_codex_runtime_reconcile_script_with_floors(minimum_version, None)
}

pub(crate) fn remote_codex_runtime_reconcile_script_with_floors(
    minimum_version: Option<&str>,
    minimum_current: Option<&StrictCurrentRuntime>,
) -> Result<String, String> {
    let minimum = match minimum_version {
        Some(value) => normalized_codex_version(value).ok_or_else(|| {
            "The pre-operation Codex version could not be normalized safely.".to_string()
        })?,
        None => String::new(),
    };
    let (minimum_current_version, minimum_current_entry, minimum_current_binary_rel) =
        match minimum_current {
            Some(current) => {
                let version = normalized_codex_version(&current.version).ok_or_else(|| {
                    "The pre-operation standalone/current version could not be normalized safely."
                        .to_string()
                })?;
                if !safe_release_entry(&current.release_entry)
                    || !release_entry_matches_version(&current.release_entry, &version)
                {
                    return Err(
                        "The pre-operation standalone/current release entry was invalid.".into(),
                    );
                }
                if !matches!(current.binary_relative_path.as_str(), "bin/codex" | "codex") {
                    return Err(
                        "The pre-operation standalone/current binary layout was invalid.".into(),
                    );
                }
                (
                    version,
                    current.release_entry.clone(),
                    current.binary_relative_path.clone(),
                )
            }
            None => (String::new(), String::new(), String::new()),
        };
    Ok(format!(
        "codexhub_minimum_version='{minimum}'\ncodexhub_minimum_current_version='{minimum_current_version}'\ncodexhub_minimum_current_entry='{minimum_current_entry}'\ncodexhub_minimum_current_binary_relative_path='{minimum_current_binary_rel}'\n{}",
        remote_codex_runtime_reconcile_script()
    ))
}

pub(crate) fn reconcile_remote_codex_runtime(
    alias: &str,
    timeout: u64,
    minimum_version: Option<&str>,
    minimum_current: Option<&StrictCurrentRuntime>,
) -> Result<CodexRuntimeReconcileResult, String> {
    let script =
        remote_codex_runtime_reconcile_script_with_floors(minimum_version, minimum_current)?;
    let output = ssh::run_ssh_script(alias, &script, timeout)
        .map_err(|_| "Could not run remote Codex runtime reconciliation.".to_string())?;
    parse_remote_codex_runtime_reconcile_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use std::io::Write as _;
    #[cfg(target_os = "linux")]
    use std::process::Stdio;

    fn output(success: bool, stdout: &str) -> ssh::SshCommandOutput {
        ssh::SshCommandOutput {
            command: "ssh test runtime".into(),
            exit_code: success.then_some(0).or(Some(1)),
            stdout: stdout.into(),
            stderr: String::new(),
            duration_ms: 1,
            timed_out: false,
        }
    }

    // Execution fixtures model the supported Linux remote, not host-side MSYS
    // or macOS filesystem and process semantics.
    #[cfg(target_os = "linux")]
    fn run_sh(script: &str) -> Option<ssh::SshCommandOutput> {
        let mut child = match std::process::Command::new("sh")
            .arg("-s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => panic!("start sh: {error}"),
        };
        child
            .stdin
            .take()
            .expect("sh stdin")
            .write_all(script.as_bytes())
            .expect("write shell fixture");
        let result = child.wait_with_output().expect("wait for shell fixture");
        Some(ssh::SshCommandOutput {
            command: "isolated sh fixture".into(),
            stdout: String::from_utf8_lossy(&result.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
            exit_code: result.status.code(),
            duration_ms: 1,
            timed_out: false,
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn run_sh(_script: &str) -> Option<ssh::SshCommandOutput> {
        None
    }

    fn run_session_cleanup_fixture(
        policy: CodexReleaseCleanupPolicy,
        comm: &str,
        arg0: &str,
        post_snapshot_mutation: &str,
    ) -> Option<ssh::SshCommandOutput> {
        run_staged_process_cleanup_fixture(
            policy,
            comm,
            "S",
            &[arg0],
            false,
            post_snapshot_mutation,
        )
    }

    fn run_staged_process_cleanup_fixture(
        policy: CodexReleaseCleanupPolicy,
        comm: &str,
        state: &str,
        argv: &[&str],
        extra_initial_task: bool,
        post_snapshot_mutation: &str,
    ) -> Option<ssh::SshCommandOutput> {
        run_staged_process_cleanup_fixture_with_task_hook(
            policy,
            comm,
            state,
            argv,
            extra_initial_task,
            post_snapshot_mutation,
            "",
        )
    }

    fn run_staged_process_cleanup_fixture_with_task_hook(
        policy: CodexReleaseCleanupPolicy,
        comm: &str,
        state: &str,
        argv: &[&str],
        extra_initial_task: bool,
        post_snapshot_mutation: &str,
        task_iteration_hook: &str,
    ) -> Option<ssh::SshCommandOutput> {
        assert!(!comm
            .chars()
            .any(|value| matches!(value, '\'' | '\n' | '\r')));
        assert!(state.len() == 1 && state.chars().all(|value| value.is_ascii_alphabetic()));
        for arg in argv {
            assert!(!arg
                .chars()
                .any(|value| matches!(value, '\'' | '\n' | '\r' | '\0')));
        }
        let cmdline_setup = if argv.is_empty() {
            r#": >"$session_dir/cmdline""#.to_string()
        } else {
            let quoted = argv
                .iter()
                .map(|arg| format!("'{arg}'"))
                .collect::<Vec<_>>()
                .join(" ");
            format!(r#"printf '%s\000' {quoted} >"$session_dir/cmdline""#)
        };
        let extra_task_setup = if extra_initial_task {
            r#"mkdir -p "$session_dir/task/611""#
        } else {
            ":"
        };
        let readlink_override = r###"readlink() {
  last_arg=""
  for readlink_arg in "$@"; do last_arg=$readlink_arg; done
  case "$last_arg" in
    */exe)
      if [ -e "${last_arg%/exe}/codexhub-test-unreadable-exe" ]; then return 1; fi
      ;;
  esac
  command readlink "$@"
}
"###;
        let snapshot_anchor =
            "scan_current_uid_executables || defer_all_direct_entries proc-identity-unknown";
        let mut generated = format!(
            "{readlink_override}{}",
            remote_codex_release_cleanup_script_with_policy(&policy).ok()?
        )
        .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        if !task_iteration_hook.is_empty() {
            let task_anchor = "zombie_task_id=${zombie_task_dir##*/}";
            assert_eq!(generated.matches(task_anchor).count(), 1);
            generated = generated.replacen(
                task_anchor,
                &format!("{task_anchor}\n{task_iteration_hook}"),
                1,
            );
        }
        assert_eq!(generated.matches(snapshot_anchor).count(), 1);
        generated = generated.replacen(
            snapshot_anchor,
            &format!("{snapshot_anchor}\n{post_snapshot_mutation}"),
            1,
        );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
hub="$home/.codex-hub"
mkdir -p "$releases" "$hub" "$home/proc"
make_release() {
  version=$1
  release="$releases/$version"
  mkdir -p "$release/bin"
  cat >"$release/bin/codex" <<CODEXHUB_TEST_BIN
#!/bin/sh
printf 'codex-cli $version\n'
CODEXHUB_TEST_BIN
  chmod 700 "$release/bin/codex"
  printf 'CodexHub managed standalone release v1\nversion=%s\n' "$version" >"$release/.codexhub-managed-release"
}
make_session() {
  session_pid=$1
  session_comm=$2
  session_starttime=$3
  session_dir="$home/proc/$session_pid"
  mkdir -p "$session_dir"
  uid=$(id -u)
  printf 'Uid:\t%s\t%s\t%s\t%s\n' "$uid" "$uid" "$uid" "$uid" >"$session_dir/status"
  printf 'Threads:\t1\n' >>"$session_dir/status"
  printf '%s (session) __SESSION_STATE__ 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 %s\n' \
    "$session_pid" "$session_starttime" >"$session_dir/stat"
  printf '%s\n' "$session_comm" >"$session_dir/comm"
  __SESSION_CMDLINE_SETUP__
  mkdir -p "$session_dir/task/$session_pid"
  __SESSION_EXTRA_TASK_SETUP__
  : >"$session_dir/codexhub-test-unreadable-exe"
}
make_release 0.142.5
make_release 0.145.0
ln -s "$releases/0.145.0" "$home/.codex/packages/standalone/current"
make_session 610 '__SESSION_COMM__' 1001
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
HOME="$home" sh "$root/cleanup.sh" >"$root/out"
cat "$root/out"
if [ -d "$releases/0.142.5" ]; then active=present; else active=absent; fi
backup=absent
for backup_release in "$hub"/deletion-backups/update-*/releases/0.142.5; do
  [ -d "$backup_release" ] || continue
  backup=present
done
printf 'CODEXHUB_TEST_OLD_ACTIVE=%s\n' "$active"
printf 'CODEXHUB_TEST_OLD_BACKUP=%s\n' "$backup"
rm -rf "$root"
"###
        .replace("__SESSION_COMM__", comm)
        .replace("__SESSION_STATE__", state)
        .replace("__SESSION_CMDLINE_SETUP__", &cmdline_setup)
        .replace("__SESSION_EXTRA_TASK_SETUP__", extra_task_setup)
        .replace("__CLEANUP_SCRIPT__", &generated);
        run_sh(&harness)
    }

    #[test]
    fn production_remote_scripts_do_not_shadow_awk_index_builtin() {
        let sources = [
            ("domain", include_str!("../domain.rs")),
            ("host operations", include_str!("host_operations.rs")),
            ("runtime", include_str!("codex_runtime.rs")),
        ];
        for (name, source) in sources {
            let production = source.split("#[cfg(test)]").next().unwrap_or(source);
            assert!(
                !production.contains("for (index ="),
                "{name} production script shadows the AWK index builtin"
            );
        }

        let runtime_production = sources[2]
            .1
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(sources[2].1);
        assert!(runtime_production.contains("old_has_prerelease = index("));
        assert!(runtime_production.contains("new_has_prerelease = index("));
    }

    #[test]
    fn reconcile_script_uses_literal_current_and_atomic_managed_files() {
        let script = remote_codex_runtime_reconcile_script();
        assert!(script.contains("candidate=\"$standalone_current\""));
        assert!(!script.contains("candidate=$(readlink -f"));
        assert!(script.contains("target_tmp=\"$target_file.codexhub.tmp.$$\""));
        assert!(script.contains("chmod 600 \"$target_tmp\""));
        assert!(script.contains("mv \"$target_tmp\" \"$target_file\""));
        assert!(script.contains("launcher_tmp=\"$launcher.codexhub.tmp.$$\""));
        assert!(script.contains("chmod 700 \"$launcher_tmp\""));
        assert!(script.contains("mv \"$launcher_tmp\" \"$launcher\""));
        assert!(script.contains("selected-target-would-downgrade"));
        assert!(script.contains("runtime-version-mismatch"));
        assert!(script.contains("captured-launcher-invalid"));
        assert!(script.contains("restore_launcher"));
        assert!(script.contains("mv \"$launcher\" \"$capture_path\""));
        assert!(script.contains("cp -P \"$launcher\" \"$launcher_backup\""));
        assert!(script.contains("standalone_release_entry \"$target_file_value\""));
        assert!(script.contains("pre_login_version=$(login_shell_version"));
        assert!(!script.contains("CODEXHUB_RUNTIME_TARGET=%s"));

        let target_backup = script
            .find("cp -p \"$target_file\" \"$target_backup\"")
            .expect("timestamped target backup");
        let target_replace = script
            .find("mv \"$target_tmp\" \"$target_file\"")
            .expect("atomic target replace");
        assert!(target_backup < target_replace);

        for armed_replace in [
            "source_moved=yes\n  if ! mv \"$launcher\" \"$capture_path\"",
            "target_changed=yes\n  if ! mv \"$target_tmp\" \"$target_file\"",
            "launcher_changed=yes\n  if ! mv \"$launcher_tmp\" \"$launcher\"",
        ] {
            assert!(
                script.contains(armed_replace),
                "rollback state must be armed before mutation: {armed_replace}"
            );
        }
    }

    #[test]
    fn reconcile_capture_uses_atomic_sidecar_and_safe_rollback_order() {
        let script = remote_codex_runtime_reconcile_script();
        for required in [
            "capture_name=\"codex-original.$timestamp.$$\"",
            "is_safe_capture_name \"$capture_name\"",
            "[ -f \"$path\" ] && [ -x \"$path\" ] && [ ! -L \"$path\" ]",
            "[ \"$capture_real\" = \"$hub_dir_real/$expected_name\" ]",
            "CodexHub managed launcher capture v1",
            "printf 'name=%s\\n' \"$expected_name\"",
            "printf 'version=%s\\n' \"$expected_version\"",
            "chmod 600 \"$capture_marker_tmp\"",
            "mv \"$capture_marker_tmp\" \"$capture_marker_path\"",
            "verify_managed_capture \"$capture_path\" \"$capture_name\"",
        ] {
            assert!(
                script.contains(required),
                "missing capture guard: {required}"
            );
        }

        let restore = script
            .split_once("restore_launcher() {")
            .and_then(|(_, tail)| tail.split_once("fail_runtime() {").map(|(body, _)| body))
            .expect("launcher rollback body");
        let moves = restore
            .match_indices("mv \"$capture_path\" \"$launcher\"")
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let marker_removals = restore
            .match_indices("rm -f \"$capture_marker_path\"")
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(moves.len(), 2);
        assert_eq!(marker_removals.len(), 2);
        assert!(moves
            .iter()
            .zip(marker_removals.iter())
            .all(|(move_index, remove_index)| move_index < remove_index));
        assert!(!script.contains("rm -rf \"$capture"));
    }

    #[test]
    fn reconcile_current_layout_count_precedes_unique_binary_validation() {
        let script = remote_codex_runtime_reconcile_script();
        let selector = script
            .split_once("select_verified_current_binary() {")
            .and_then(|(_, tail)| {
                tail.split_once("managed_release_marker_valid() {")
                    .map(|(body, _)| body)
            })
            .expect("current selector");
        let direct_count = selector
            .find("current_match_count=$((current_match_count + 1))")
            .expect("direct executable layout count");
        let existence_count = selector
            .find("if [ -e \"$current_direct\" ] || [ -L \"$current_direct\" ]; then")
            .expect("all current direct paths count toward ambiguity");
        let unique_guard = selector
            .find("[ \"$current_match_count\" -eq 1 ] || return 1")
            .expect("unique executable layout guard");
        let identity_check = selector
            .find("verify_direct_release_target \"$saved_current_direct\"")
            .expect("unique layout identity check");
        assert!(existence_count < direct_count && direct_count < unique_guard);
        assert!(unique_guard < identity_check);
        assert!(script.contains(
            "select_verified_current_binary || fail_runtime current-release-identity-unknown"
        ));

        let cleanup = remote_codex_release_cleanup_script();
        let cleanup_selector = cleanup
            .split_once("select_release_binary() {")
            .and_then(|(_, tail)| {
                tail.split_once("verify_candidate() {")
                    .map(|(body, _)| body)
            })
            .expect("cleanup release selector");
        let cleanup_count = cleanup_selector
            .find("release_binary_match_count=$((release_binary_match_count + 1))")
            .expect("cleanup direct layout count");
        let cleanup_existence = cleanup_selector
            .find("if [ -e \"$binary\" ] || [ -L \"$binary\" ]; then")
            .expect("all cleanup direct paths count toward ambiguity");
        let cleanup_identity = cleanup_selector
            .find("binary_version=$(normalized_version_for_binary \"$saved_release_binary\"")
            .expect("cleanup unique layout identity check");
        assert!(cleanup_existence < cleanup_count && cleanup_count < cleanup_identity);
    }

    #[test]
    fn every_direct_release_verification_rejects_dual_layouts_before_mutation() {
        let script = remote_codex_runtime_reconcile_script();
        let verifier = script
            .split_once("verify_direct_release_target() {")
            .and_then(|(_, tail)| {
                tail.split_once("select_verified_current_binary() {")
                    .map(|(body, _)| body)
            })
            .expect("direct release verifier");
        let layout_count = verifier
            .find("direct_layout_count=$((direct_layout_count + 1))")
            .expect("direct layout count");
        let existence_count = verifier
            .find("if [ -e \"$direct_binary\" ] || [ -L \"$direct_binary\" ]; then")
            .expect("all direct paths count toward ambiguity");
        let unique_guard = verifier
            .find("[ \"$direct_layout_count\" -eq 1 ] || return 1")
            .expect("direct layout unique guard");
        let version_probe = verifier
            .find("binary_version=$(normalized_version_for_path")
            .expect("binary version probe");
        assert!(existence_count < layout_count);
        assert!(layout_count < unique_guard && unique_guard < version_probe);

        let writer = REMOTE_CODEX_RUNTIME_WRITER_LOCK_PRELUDE;
        for required in [
            "if [ -e \"$current_binary\" ] || [ -L \"$current_binary\" ]; then",
            "if [ -e \"$locked_direct\" ] || [ -L \"$locked_direct\" ]; then",
            "[ \"$locked_binary_real\" = \"$locked_dir_real/$codexhub_locked_current_binary_relative_path\" ] || return 1",
        ] {
            assert!(writer.contains(required), "missing writer layout guard: {required}");
        }
        for required in [
            "{ [ -e \"$release_dir/bin/codex\" ] || [ -L \"$release_dir/bin/codex\" ]; }",
            "{ [ -e \"$release_dir/codex\" ] || [ -L \"$release_dir/codex\" ]; }",
        ] {
            assert!(
                REMOTE_STRICT_CURRENT_VERSION_SCRIPT.contains(required),
                "missing strict-current layout guard: {required}"
            );
        }

        let target_rejection = script
            .find("verify_direct_release_target \"$target_file_value\" || fail_runtime target-release-identity-unknown")
            .expect("direct target rejection");
        let target_stage = script
            .find("target_tmp=\"$target_file.codexhub.tmp.$$\"")
            .expect("target stage");
        assert!(target_rejection < target_stage);

        let executable_validator = script
            .split_once("is_valid_executable_target() {")
            .and_then(|(_, tail)| {
                tail.split_once("release_entry_matches_version() {")
                    .map(|(body, _)| body)
            })
            .expect("executable target validator");
        let original_path_capture = executable_validator
            .find("codexhub_target_validation_path=$1")
            .expect("original target path capture");
        let canonicalize = executable_validator
            .find("target_real=$(readlink -f \"$codexhub_target_validation_path\"")
            .expect("unconditional canonical target");
        let canonical_direct_check = executable_validator
            .find("standalone_release_entry \"$direct_target\"")
            .expect("canonical direct release check");
        assert!(canonicalize < canonical_direct_check);

        let managed_checks = executable_validator
            .match_indices("is_managed_launcher_path")
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(managed_checks.len(), 2);
        assert!(original_path_capture < managed_checks[0]);
        let same_binary_guard = executable_validator
            .find("if [ \"$target_real\" = \"$launcher_real\" ]; then")
            .expect("same-binary launcher guard");
        let symlink_gate = executable_validator
            .find("[ -L \"$launcher\" ] || return 1")
            .expect("native launcher symlink gate");
        let current_recheck = executable_validator
            .find("select_verified_current_binary || return 1")
            .expect("verified current recheck");
        let literal_current_gate = executable_validator
            .find("[ \"$codexhub_same_binary_target_path\" = \"$standalone_current\" ] || return 1")
            .expect("literal current target gate");
        let canonical_current_recheck = executable_validator
            .find("codexhub_same_binary_current_real=$(readlink -f \"$standalone_current\"")
            .expect("canonical current recheck");
        let unchanged_identity_gate = executable_validator
            .find("[ \"$codexhub_same_binary_current_real\" = \"$codexhub_same_binary_target_real\" ] || return 1")
            .expect("unchanged current identity gate");
        assert!(managed_checks[1] < same_binary_guard);
        assert!(same_binary_guard < symlink_gate);
        assert!(symlink_gate < current_recheck);
        assert!(current_recheck < literal_current_gate);
        assert!(literal_current_gate < canonical_current_recheck);
        assert!(canonical_current_recheck < unchanged_identity_gate);
        assert!(unchanged_identity_gate < canonical_direct_check);
        assert!(script.contains("[ \"$value\" != \"$launcher\" ] || return 1"));
        assert!(
            !executable_validator.contains("[ \"$target_real\" = \"$launcher_real\" ] && return 0")
        );
    }

    #[test]
    fn exact_current_recovery_precedes_login_and_all_candidate_floor_rejections() {
        let script = remote_codex_runtime_reconcile_script();
        let exact_identity = script
            .find("floor_binary=\"$release_root/$minimum_current_entry/$minimum_current_binary_relative_path\"")
            .expect("exact floor identity");
        let current_guard = script
            .find("current_verified=no")
            .expect("installer-mutated current guard");
        let login_probe = script
            .find("# Capture the login-shell-visible runtime only after exact current recovery")
            .expect("login probe ordering");
        let candidate_floor = script
            .find("enforce_candidate_floor \"$before_version\"")
            .expect("active runtime floor");
        let ordinary_minimum = script
            .find("enforce_candidate_floor \"$normalized_minimum\"")
            .expect("ordinary operation floor");
        assert!(exact_identity < current_guard);
        assert!(current_guard < login_probe);
        assert!(login_probe < candidate_floor && candidate_floor < ordinary_minimum);
        assert!(script.contains(
            "prefer_exact_current_candidate || fail_runtime exact-current-restore-failed"
        ));
    }

    #[test]
    fn reconcile_script_is_posix_and_does_not_use_broad_process_commands() {
        let script = remote_codex_runtime_reconcile_script();
        for forbidden in [
            "[[",
            "pkill",
            "killall",
            "kill -9",
            "SIGKILL",
            "<( ",
            "readlink -f \"$launcher\" >",
        ] {
            assert!(
                !script.contains(forbidden),
                "found forbidden token {forbidden}"
            );
        }
        assert!(!script.contains("sk-test"));
        assert!(!script.contains("OPENAI_API_KEY"));
        assert!(!script.contains("rm -rf"));
    }

    #[test]
    fn reconcile_output_requires_consistent_three_way_versions() {
        let parsed = parse_remote_codex_runtime_reconcile_output(&output(
            true,
            "CODEXHUB_RUNTIME_STATUS=coordinated\n\
             CODEXHUB_RUNTIME_TARGET_CHANGED=yes\n\
             CODEXHUB_RUNTIME_LAUNCHER_CHANGED=yes\n\
             CODEXHUB_RUNTIME_TARGET_VERSION=0.144.5\n\
             CODEXHUB_RUNTIME_LAUNCHER_VERSION=0.144.5\n\
             CODEXHUB_RUNTIME_LOGIN_VERSION=0.144.5\n\
             CODEXHUB_RUNTIME_RELEASE_MARKED=yes\n\
             CODEXHUB_RUNTIME_REASON=runtime-coordinated",
        ))
        .expect("valid runtime result");
        assert!(parsed.completed());
        assert_eq!(parsed.status, CodexRuntimeReconcileStatus::Coordinated);

        let mismatch = output(
            true,
            "CODEXHUB_RUNTIME_STATUS=unchanged\n\
             CODEXHUB_RUNTIME_TARGET_CHANGED=no\n\
             CODEXHUB_RUNTIME_LAUNCHER_CHANGED=no\n\
             CODEXHUB_RUNTIME_TARGET_VERSION=0.144.5\n\
             CODEXHUB_RUNTIME_LAUNCHER_VERSION=0.142.5\n\
             CODEXHUB_RUNTIME_LOGIN_VERSION=0.144.5\n\
             CODEXHUB_RUNTIME_RELEASE_MARKED=no\n\
             CODEXHUB_RUNTIME_REASON=runtime-already-coordinated",
        );
        assert!(parse_remote_codex_runtime_reconcile_output(&mismatch).is_err());
    }

    #[test]
    fn reconcile_output_rejects_duplicate_or_unsafe_markers() {
        let duplicate = output(
            true,
            "CODEXHUB_RUNTIME_STATUS=unchanged\n\
             CODEXHUB_RUNTIME_STATUS=unchanged\n\
             CODEXHUB_RUNTIME_TARGET_CHANGED=no\n\
             CODEXHUB_RUNTIME_LAUNCHER_CHANGED=no\n\
             CODEXHUB_RUNTIME_TARGET_VERSION=0.144.5\n\
             CODEXHUB_RUNTIME_LAUNCHER_VERSION=0.144.5\n\
             CODEXHUB_RUNTIME_LOGIN_VERSION=0.144.5\n\
             CODEXHUB_RUNTIME_RELEASE_MARKED=no\n\
             CODEXHUB_RUNTIME_REASON=runtime-already-coordinated",
        );
        assert!(parse_remote_codex_runtime_reconcile_output(&duplicate).is_err());

        let unsafe_reason = output(
            false,
            "CODEXHUB_RUNTIME_STATUS=manual-required\n\
             CODEXHUB_RUNTIME_TARGET_CHANGED=no\n\
             CODEXHUB_RUNTIME_LAUNCHER_CHANGED=no\n\
             CODEXHUB_RUNTIME_TARGET_VERSION=\n\
             CODEXHUB_RUNTIME_LAUNCHER_VERSION=\n\
             CODEXHUB_RUNTIME_LOGIN_VERSION=\n\
             CODEXHUB_RUNTIME_RELEASE_MARKED=no\n\
             CODEXHUB_RUNTIME_REASON=contains secret text",
        );
        assert!(parse_remote_codex_runtime_reconcile_output(&unsafe_reason).is_err());
    }

    #[test]
    fn cleanup_protocol_is_counted_and_rejects_contradictions() {
        let parsed = parse_remote_codex_release_cleanup_output(&output(
            true,
            "CODEXHUB_CLEANUP_STATUS=completed\n\
             CODEXHUB_CLEANUP_SCANNED=4\n\
             CODEXHUB_CLEANUP_ADOPTED=1\n\
             CODEXHUB_CLEANUP_REMOVED=1\n\
             CODEXHUB_CLEANUP_BACKED_UP=1\n\
             CODEXHUB_CLEANUP_BACKUP_ID=update-20260719120000-42\n\
             CODEXHUB_CLEANUP_IGNORED_SESSION_PROCESSES=3\n\
             CODEXHUB_CLEANUP_RETAINED=3\n\
             CODEXHUB_CLEANUP_DEFERRED=0\n\
             CODEXHUB_CLEANUP_REASON=cleanup-complete",
        ))
        .expect("valid cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Completed);
        assert_eq!(parsed.adopted_count, 1);
        assert_eq!(parsed.removed_count, 1);
        assert_eq!(parsed.backed_up_count, 1);
        assert_eq!(parsed.ignored_session_process_count, 3);
        assert_eq!(
            parsed.backup_id.as_deref(),
            Some("update-20260719120000-42")
        );
        assert!(!parsed.safe_summary().contains('/'));

        let deferred = parse_remote_codex_release_cleanup_output(&output(
            true,
            "CODEXHUB_CLEANUP_STATUS=deferred\n\
             CODEXHUB_CLEANUP_SCANNED=1\n\
             CODEXHUB_CLEANUP_ADOPTED=1\n\
             CODEXHUB_CLEANUP_REMOVED=0\n\
             CODEXHUB_CLEANUP_BACKED_UP=0\n\
             CODEXHUB_CLEANUP_BACKUP_ID=none\n\
             CODEXHUB_CLEANUP_IGNORED_SESSION_PROCESSES=0\n\
             CODEXHUB_CLEANUP_RETAINED=0\n\
             CODEXHUB_CLEANUP_DEFERRED=1\n\
             CODEXHUB_CLEANUP_REASON=proc-identity-unknown",
        ))
        .expect("safe deferred cleanup");
        assert!(!deferred.hard_failed());

        let contradictory = output(
            true,
            "CODEXHUB_CLEANUP_STATUS=completed\n\
             CODEXHUB_CLEANUP_SCANNED=4\n\
             CODEXHUB_CLEANUP_ADOPTED=0\n\
             CODEXHUB_CLEANUP_REMOVED=1\n\
             CODEXHUB_CLEANUP_BACKED_UP=0\n\
             CODEXHUB_CLEANUP_BACKUP_ID=none\n\
             CODEXHUB_CLEANUP_IGNORED_SESSION_PROCESSES=0\n\
             CODEXHUB_CLEANUP_RETAINED=1\n\
             CODEXHUB_CLEANUP_DEFERRED=1\n\
             CODEXHUB_CLEANUP_REASON=cleanup-complete",
        );
        assert!(parse_remote_codex_release_cleanup_output(&contradictory).is_err());

        let impossible_adoption = output(
            true,
            "CODEXHUB_CLEANUP_STATUS=completed\n\
             CODEXHUB_CLEANUP_SCANNED=1\n\
             CODEXHUB_CLEANUP_ADOPTED=2\n\
             CODEXHUB_CLEANUP_REMOVED=1\n\
             CODEXHUB_CLEANUP_BACKED_UP=1\n\
             CODEXHUB_CLEANUP_BACKUP_ID=update-20260719120000-42\n\
             CODEXHUB_CLEANUP_IGNORED_SESSION_PROCESSES=0\n\
             CODEXHUB_CLEANUP_RETAINED=0\n\
             CODEXHUB_CLEANUP_DEFERRED=0\n\
             CODEXHUB_CLEANUP_REASON=cleanup-complete",
        );
        assert!(parse_remote_codex_release_cleanup_output(&impossible_adoption).is_err());

        let impossible_backup = output(
            true,
            "CODEXHUB_CLEANUP_STATUS=completed\n\
             CODEXHUB_CLEANUP_SCANNED=1\n\
             CODEXHUB_CLEANUP_ADOPTED=1\n\
             CODEXHUB_CLEANUP_REMOVED=1\n\
             CODEXHUB_CLEANUP_BACKED_UP=2\n\
             CODEXHUB_CLEANUP_BACKUP_ID=update-20260719120000-42\n\
             CODEXHUB_CLEANUP_IGNORED_SESSION_PROCESSES=0\n\
             CODEXHUB_CLEANUP_RETAINED=0\n\
             CODEXHUB_CLEANUP_DEFERRED=0\n\
             CODEXHUB_CLEANUP_REASON=cleanup-complete",
        );
        assert!(parse_remote_codex_release_cleanup_output(&impossible_backup).is_err());

        let missing_backup_id = output(
            true,
            "CODEXHUB_CLEANUP_STATUS=completed\n\
             CODEXHUB_CLEANUP_SCANNED=1\n\
             CODEXHUB_CLEANUP_ADOPTED=1\n\
             CODEXHUB_CLEANUP_REMOVED=1\n\
             CODEXHUB_CLEANUP_BACKED_UP=1\n\
             CODEXHUB_CLEANUP_BACKUP_ID=none\n\
             CODEXHUB_CLEANUP_IGNORED_SESSION_PROCESSES=0\n\
             CODEXHUB_CLEANUP_RETAINED=0\n\
             CODEXHUB_CLEANUP_DEFERRED=0\n\
             CODEXHUB_CLEANUP_REASON=cleanup-complete",
        );
        assert!(parse_remote_codex_release_cleanup_output(&missing_backup_id).is_err());

        let overflow = output(
            true,
            "CODEXHUB_CLEANUP_STATUS=completed\n\
             CODEXHUB_CLEANUP_SCANNED=4294967295\n\
             CODEXHUB_CLEANUP_ADOPTED=0\n\
             CODEXHUB_CLEANUP_REMOVED=4294967295\n\
             CODEXHUB_CLEANUP_BACKED_UP=0\n\
             CODEXHUB_CLEANUP_BACKUP_ID=none\n\
             CODEXHUB_CLEANUP_IGNORED_SESSION_PROCESSES=0\n\
             CODEXHUB_CLEANUP_RETAINED=1\n\
             CODEXHUB_CLEANUP_DEFERRED=0\n\
             CODEXHUB_CLEANUP_REASON=cleanup-complete",
        );
        let error = parse_remote_codex_release_cleanup_output(&overflow)
            .expect_err("overflowed cleanup counters must be rejected");
        assert!(error.contains("overflow"));
    }

    #[test]
    fn strict_current_probe_requires_consistent_safe_markers() {
        let available = parse_remote_strict_current_version_output(&output(
            true,
            "CODEXHUB_CURRENT_STATUS=available\n\
             CODEXHUB_CURRENT_VERSION=0.144.5\n\
             CODEXHUB_CURRENT_RELEASE_ENTRY=0.144.5\n\
             CODEXHUB_CURRENT_BINARY_REL=bin/codex\n\
             CODEXHUB_CURRENT_REASON=current-runtime-verified",
        ))
        .expect("strict current version");
        assert_eq!(
            available.as_ref().map(|value| value.version.as_str()),
            Some("0.144.5")
        );
        assert_eq!(
            available.as_ref().map(|value| value.release_entry.as_str()),
            Some("0.144.5")
        );

        let suffix = parse_remote_strict_current_version_output(&output(
            true,
            "CODEXHUB_CURRENT_STATUS=available\n\
             CODEXHUB_CURRENT_VERSION=0.145.0-alpha.1\n\
             CODEXHUB_CURRENT_RELEASE_ENTRY=0.145.0-alpha.1-x86_64-unknown-linux-musl\n\
             CODEXHUB_CURRENT_BINARY_REL=codex\n\
             CODEXHUB_CURRENT_REASON=current-runtime-verified",
        ))
        .expect("official suffix current")
        .expect("suffix runtime");
        assert_eq!(suffix.version, "0.145.0-alpha.1");
        assert_eq!(suffix.binary_relative_path, "codex");

        for bad_entry in [
            "0.144.50-x86_64-unknown-linux-musl",
            "0.144.5/../../unsafe",
            "unrelated-x86_64",
        ] {
            let invalid = output(
                true,
                &format!(
                    "CODEXHUB_CURRENT_STATUS=available\nCODEXHUB_CURRENT_VERSION=0.144.5\nCODEXHUB_CURRENT_RELEASE_ENTRY={bad_entry}\nCODEXHUB_CURRENT_BINARY_REL=bin/codex\nCODEXHUB_CURRENT_REASON=current-runtime-verified"
                ),
            );
            assert!(parse_remote_strict_current_version_output(&invalid).is_err());
        }

        let failed = output(
            false,
            "CODEXHUB_CURRENT_STATUS=failed\n\
             CODEXHUB_CURRENT_VERSION=\n\
             CODEXHUB_CURRENT_RELEASE_ENTRY=\n\
             CODEXHUB_CURRENT_BINARY_REL=\n\
             CODEXHUB_CURRENT_REASON=current-version-mismatch",
        );
        assert!(parse_remote_strict_current_version_output(&failed).is_err());
    }

    #[test]
    fn strict_current_probe_rejects_a_bad_second_direct_layout() {
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
release="$home/.codex/packages/standalone/releases/0.145.0"
mkdir -p "$release/bin"
cat >"$release/bin/codex" <<'CODEXHUB_BIN'
#!/bin/sh
printf 'codex-cli 0.145.0\n'
CODEXHUB_BIN
chmod 700 "$release/bin/codex"
ln -s bin/codex "$release/codex"
ln -s "$release" "$home/.codex/packages/standalone/current"
cat >"$root/probe.sh" <<'CODEXHUB_PROBE_SCRIPT'
__PROBE_SCRIPT__
CODEXHUB_PROBE_SCRIPT
set +e
HOME="$home" sh "$root/probe.sh" >"$root/out"
status=$?
set -e
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
rm -rf "$root"
"###
        .replace("__PROBE_SCRIPT__", REMOTE_STRICT_CURRENT_VERSION_SCRIPT);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "strict current fixture failed: {}",
            output.stderr
        );
        let error = parse_remote_strict_current_version_output(&ssh::SshCommandOutput {
            exit_code: Some(1),
            ..output.clone()
        })
        .expect_err("a second symlink layout must be ambiguous");
        assert!(error.contains("current-binary-layout-ambiguous"));
        assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=1"));
    }

    #[test]
    fn version_normalization_accepts_codex_labels_and_rejects_unsafe_values() {
        assert_eq!(
            normalized_codex_version("codex-cli 0.144.5").as_deref(),
            Some("0.144.5")
        );
        assert_eq!(
            normalized_codex_version("v0.144.5-beta.1+build").as_deref(),
            Some("0.144.5-beta.1+build")
        );
        for unsafe_value in ["", "codex", "1", "1.2/3", "1.2\nsecret"] {
            assert!(normalized_codex_version(unsafe_value).is_none());
        }

        let floors = remote_version_floor_prelude(Some("codex-cli 0.143.0"), Some("0.144.5"))
            .expect("safe update floors");
        assert!(floors.contains("codexhub_minimum_version='0.143.0'"));
        assert!(floors.contains("codexhub_minimum_current_version='0.144.5'"));
        assert!(floors.contains("codexhub_version_meets_floors"));
        assert!(floors.contains("${codexhub_locked_runtime_floor:-}"));
        assert!(remote_version_floor_prelude(Some("0.143.0; secret"), None).is_err());
    }

    #[test]
    fn reconcile_recovers_overwritten_launcher_and_profile_retry_keeps_new_version() {
        let first = remote_codex_runtime_reconcile_script_with_minimum(Some("codex-cli 0.142.5"))
            .expect("minimum reconcile script");
        let second = remote_codex_runtime_reconcile_script();
        let harness = r###"set -u
root=$(mktemp -d)
home="$root/home"
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$home/.codex/packages/standalone/releases/0.142.5/bin"
cat >"$home/.codex/packages/standalone/releases/0.142.5/bin/codex" <<'CODEXHUB_OLD_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_OLD_BIN
cat >"$home/.local/bin/codex" <<'CODEXHUB_NEW_BIN'
#!/bin/sh
printf 'codex-cli 0.144.5\n'
CODEXHUB_NEW_BIN
chmod 700 "$home/.codex/packages/standalone/releases/0.142.5/bin/codex" "$home/.local/bin/codex"
printf '%s\n' "$home/.codex/packages/standalone/releases/0.142.5/bin/codex" >"$home/.codex-hub/codex-target"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/first.sh" <<'CODEXHUB_FIRST_SCRIPT'
__FIRST_SCRIPT__
CODEXHUB_FIRST_SCRIPT
cat >"$root/second.sh" <<'CODEXHUB_SECOND_SCRIPT'
__SECOND_SCRIPT__
CODEXHUB_SECOND_SCRIPT
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/first.sh" >"$root/first.out"
first_status=$?
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/second.sh" >"$root/second.out"
second_status=$?
cat "$root/first.out"
printf 'CODEXHUB_TEST_PROFILE_RETRY\n'
cat "$root/second.out"
if grep -F 'CodexHub managed launcher' "$home/.local/bin/codex" >/dev/null 2>&1; then launcher=managed; else launcher=external; fi
target=$(sed -n '1p' "$home/.codex-hub/codex-target")
target_version=$("$target" --version | awk '{print $NF}')
printf 'CODEXHUB_TEST_FIRST_STATUS=%s\n' "$first_status"
printf 'CODEXHUB_TEST_SECOND_STATUS=%s\n' "$second_status"
printf 'CODEXHUB_TEST_LAUNCHER=%s\n' "$launcher"
printf 'CODEXHUB_TEST_TARGET_VERSION=%s\n' "$target_version"
rm -rf "$root"
"###
        .replace("__FIRST_SCRIPT__", &first)
        .replace("__SECOND_SCRIPT__", second);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "runtime fixture failed: {}",
            output.stderr
        );
        let (first_output, retry_output) = output
            .stdout
            .split_once("CODEXHUB_TEST_PROFILE_RETRY\n")
            .expect("profile retry output");
        let first_result = parse_remote_codex_runtime_reconcile_output(&ssh::SshCommandOutput {
            stdout: first_output.into(),
            ..output.clone()
        })
        .expect("updated runtime result");
        let retry_result = parse_remote_codex_runtime_reconcile_output(&ssh::SshCommandOutput {
            stdout: retry_output.into(),
            ..output.clone()
        })
        .expect("profile retry runtime result");
        assert_eq!(first_result.target_version.as_deref(), Some("0.144.5"));
        assert_eq!(retry_result.target_version.as_deref(), Some("0.144.5"));
        assert!(retry_output.contains("CODEXHUB_TEST_FIRST_STATUS=0"));
        assert!(retry_output.contains("CODEXHUB_TEST_SECOND_STATUS=0"));
        assert!(retry_output.contains("CODEXHUB_TEST_LAUNCHER=managed"));
        assert!(retry_output.contains("CODEXHUB_TEST_TARGET_VERSION=0.144.5"));
    }

    #[test]
    fn reconcile_marks_strict_legacy_target_owned_by_managed_launcher() {
        let generated = remote_codex_runtime_reconcile_script();
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$releases/0.142.5/bin" "$releases/0.144.5/bin"
for version in 0.142.5 0.144.5; do
  cat >"$releases/$version/bin/codex" <<CODEXHUB_TEST_BIN
#!/bin/sh
printf 'codex-cli $version\n'
CODEXHUB_TEST_BIN
  chmod 700 "$releases/$version/bin/codex"
done
ln -s "$releases/0.144.5" "$home/.codex/packages/standalone/current"
printf '%s\n' "$releases/0.142.5/bin/codex" >"$home/.codex-hub/codex-target"
cat >"$home/.local/bin/codex" <<'CODEXHUB_MANAGED_LAUNCHER'
#!/bin/sh
# CodexHub managed launcher: loads remote API env before running real Codex.
target=$(sed -n '1p' "$HOME/.codex-hub/codex-target")
exec "$target" "$@"
CODEXHUB_MANAGED_LAUNCHER
chmod 700 "$home/.local/bin/codex"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
cat "$root/out"
for version in 0.142.5 0.144.5; do
  if [ -f "$releases/$version/.codexhub-managed-release" ]; then state=marked; else state=missing; fi
  printf 'CODEXHUB_TEST_MARKER_%s=%s\n' "$version" "$state"
done
rm -rf "$root"
"###
        .replace("__RECONCILE_SCRIPT__", generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "legacy marker fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_runtime_reconcile_output(&output)
            .expect("legacy target reconcile result");
        assert_eq!(parsed.target_version.as_deref(), Some("0.144.5"));
        assert!(output
            .stdout
            .contains("CODEXHUB_TEST_MARKER_0.142.5=marked"));
        assert!(output
            .stdout
            .contains("CODEXHUB_TEST_MARKER_0.144.5=marked"));
    }

    #[test]
    fn reconcile_rejects_runtime_below_operation_start_version() {
        let generated = remote_codex_runtime_reconcile_script_with_minimum(Some("0.142.5"))
            .expect("minimum reconcile script");
        let harness = r###"set -u
root=$(mktemp -d)
home="$root/home"
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$home/.codex/packages/standalone/releases/0.142.5/bin"
for item in "0.141.0:$home/.local/bin/codex" "0.142.5:$home/.codex/packages/standalone/releases/0.142.5/bin/codex"
do
  version=${item%%:*}
  path=${item#*:}
  cat >"$path" <<CODEXHUB_BIN
#!/bin/sh
printf 'codex-cli $version\\n'
CODEXHUB_BIN
  chmod 700 "$path"
done
printf '%s\n' "$home/.codex/packages/standalone/releases/0.142.5/bin/codex" >"$home/.codex-hub/codex-target"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
status=$?
cat "$root/out"
printf 'CODEXHUB_TEST_SCRIPT_STATUS=%s\n' "$status"
printf 'CODEXHUB_TEST_LAUNCHER_VERSION=%s\n' "$("$home/.local/bin/codex" --version | awk '{print $NF}')"
rm -rf "$root"
exit 0
"###
        .replace("__RECONCILE_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "downgrade fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_runtime_reconcile_output(&ssh::SshCommandOutput {
            stdout: output.stdout.clone(),
            exit_code: Some(1),
            ..output.clone()
        })
        .expect("manual repair downgrade result");
        assert_eq!(parsed.status, CodexRuntimeReconcileStatus::ManualRequired);
        assert_eq!(parsed.reason, "runtime-version-below-operation-start");
        assert!(output.stdout.contains("CODEXHUB_TEST_SCRIPT_STATUS=1"));
        assert!(output
            .stdout
            .contains("CODEXHUB_TEST_LAUNCHER_VERSION=0.141.0"));
    }

    #[test]
    fn reconcile_never_replaces_a_higher_login_runtime_with_stale_current() {
        let generated = remote_codex_runtime_reconcile_script();
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
old="$home/.codex/packages/standalone/releases/0.142.5"
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$home/global" "$old/bin"
cat >"$old/bin/codex" <<'CODEXHUB_OLD_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_OLD_BIN
cat >"$home/global/codex" <<'CODEXHUB_HIGH_BIN'
#!/bin/sh
printf 'codex-cli 0.144.5\n'
CODEXHUB_HIGH_BIN
chmod 700 "$old/bin/codex" "$home/global/codex"
ln -s "$old" "$home/.codex/packages/standalone/current"
printf '%s\n' "$old/bin/codex" >"$home/.codex-hub/codex-target"
printf '%s\n' 'PATH="$HOME/.local/bin:$HOME/global:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
set +e
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$home/global:$PATH" sh "$root/reconcile.sh" >"$root/out"
status=$?
set -e
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
if [ -e "$home/.local/bin/codex" ] || [ -L "$home/.local/bin/codex" ]; then launcher=present; else launcher=absent; fi
printf 'CODEXHUB_TEST_LAUNCHER=%s\n' "$launcher"
rm -rf "$root"
"###
        .replace("__RECONCILE_SCRIPT__", generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "pre-login fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_runtime_reconcile_output(&ssh::SshCommandOutput {
            exit_code: Some(1),
            ..output.clone()
        })
        .expect("manual pre-login result");
        assert_eq!(parsed.status, CodexRuntimeReconcileStatus::ManualRequired);
        assert_eq!(parsed.reason, "selected-target-below-locked-runtime");
        assert!(output.stdout.contains("CODEXHUB_TEST_LAUNCHER=absent"));
    }

    #[test]
    fn reconcile_restores_verified_current_when_installer_candidate_is_stale() {
        let current_floor = StrictCurrentRuntime {
            version: "0.144.5".into(),
            release_entry: "0.144.5".into(),
            binary_relative_path: "bin/codex".into(),
        };
        let generated = remote_codex_runtime_reconcile_script_with_floors(
            Some("0.144.5"),
            Some(&current_floor),
        )
        .expect("two-floor reconcile script");
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$releases/0.142.5/bin" "$releases/0.143.0/bin" "$releases/0.144.5/bin"
for version in 0.142.5 0.143.0 0.144.5; do
  cat >"$releases/$version/bin/codex" <<CODEXHUB_TEST_BIN
#!/bin/sh
printf 'codex-cli $version\n'
CODEXHUB_TEST_BIN
  chmod 700 "$releases/$version/bin/codex"
done
# Simulate a stale installer replacing current and the public launcher.
ln -s "$releases/0.143.0" "$home/.codex/packages/standalone/current"
ln -s "$releases/0.143.0/bin/codex" "$home/.local/bin/codex"
printf '%s\n' "$releases/0.142.5/bin/codex" >"$home/.codex-hub/codex-target"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
cat "$root/out"
printf 'CODEXHUB_TEST_CURRENT=%s\n' "$(readlink -f "$home/.codex/packages/standalone/current")"
printf 'CODEXHUB_TEST_TARGET=%s\n' "$(sed -n '1p' "$home/.codex-hub/codex-target")"
rm -rf "$root"
"###
        .replace("__RECONCILE_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "stale installer fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_runtime_reconcile_output(&output)
            .expect("restored current reconcile result");
        assert_eq!(parsed.target_version.as_deref(), Some("0.144.5"));
        assert!(output.stdout.contains("CODEXHUB_TEST_CURRENT="));
        assert!(output.stdout.contains("/releases/0.144.5"));
        assert!(output
            .stdout
            .contains("/.codex/packages/standalone/current/bin/codex"));
    }

    #[test]
    fn reconcile_accepts_native_launcher_aliasing_the_verified_current_binary() {
        let generated = remote_codex_runtime_reconcile_script();
        for (layout_name, relative) in [("legacy", "bin/codex"), ("official", "codex")] {
            let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
release="$home/.codex/packages/standalone/releases/0.144.6"
relative='__RELATIVE__'
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$release"
case "$relative" in bin/codex) mkdir -p "$release/bin" ;; codex) ;; *) exit 97 ;; esac
cat >"$release/$relative" <<'CODEXHUB_NATIVE_BIN'
#!/bin/sh
printf 'codex-cli 0.144.6\n'
CODEXHUB_NATIVE_BIN
chmod 700 "$release/$relative"
printf 'CodexHub managed standalone release v1\nversion=0.144.6\n' >"$release/.codexhub-managed-release"
ln -s "$release" "$home/.codex/packages/standalone/current"
# Native installers leave the public command as a symlink to the same verified binary.
ln -s "$release/$relative" "$home/.local/bin/codex"
printf '%s\n' "$home/.codex/packages/standalone/current/$relative" >"$home/.codex-hub/codex-target"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
set +e
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
status=$?
set -e
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
printf 'CODEXHUB_TEST_TARGET=%s\n' "$(sed -n '1p' "$home/.codex-hub/codex-target")"
if [ -L "$home/.local/bin/codex" ]; then launcher=symlink; elif [ -f "$home/.local/bin/codex" ]; then launcher=regular; else launcher=missing; fi
printf 'CODEXHUB_TEST_LAUNCHER=%s\n' "$launcher"
printf 'CODEXHUB_TEST_MANAGED=%s\n' "$(if [ "$(sed -n '2p' "$home/.local/bin/codex")" = '# CodexHub managed launcher: loads remote API env before running real Codex.' ]; then printf yes; else printf no; fi)"
printf 'CODEXHUB_TEST_VERSION=%s\n' "$(HOME="$home" "$home/.local/bin/codex" --version | awk '{ print $NF }')"
rm -rf "$root"
exit "$status"
"###
            .replace("__RELATIVE__", relative)
            .replace("__RECONCILE_SCRIPT__", generated);
            let Some(output) = run_sh(&harness) else {
                return;
            };
            assert!(
                output.success(),
                "{layout_name} native launcher alias fixture failed: stdout={} stderr={}",
                output.stdout,
                output.stderr
            );
            let parsed = parse_remote_codex_runtime_reconcile_output(&output)
                .expect("native launcher alias reconcile result");
            assert_eq!(parsed.status, CodexRuntimeReconcileStatus::Coordinated);
            assert_eq!(parsed.target_version.as_deref(), Some("0.144.6"));
            assert!(output.stdout.contains("CODEXHUB_TEST_TARGET="));
            assert!(output
                .stdout
                .contains(&format!("/.codex/packages/standalone/current/{relative}")));
            assert!(output.stdout.contains("CODEXHUB_TEST_LAUNCHER=regular"));
            assert!(output.stdout.contains("CODEXHUB_TEST_MANAGED=yes"));
            assert!(output.stdout.contains("CODEXHUB_TEST_VERSION=0.144.6"));
        }
    }

    #[test]
    fn reconcile_rejects_an_indirect_alias_through_the_public_launcher() {
        let generated = remote_codex_runtime_reconcile_script();
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
release="$home/.codex/packages/standalone/releases/0.144.6"
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$release/bin"
cat >"$release/bin/codex" <<'CODEXHUB_NATIVE_BIN'
#!/bin/sh
printf 'codex-cli 0.144.6\n'
CODEXHUB_NATIVE_BIN
chmod 700 "$release/bin/codex"
ln -s "$release" "$home/.codex/packages/standalone/current"
ln -s "$release/bin/codex" "$home/.local/bin/codex"
ln -s "$home/.local/bin/codex" "$home/launcher-alias"
printf '%s\n' "$home/launcher-alias" >"$home/.codex-hub/codex-target"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
set +e
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
status=$?
set -e
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
if [ -L "$home/.local/bin/codex" ]; then launcher=preserved; else launcher=changed; fi
printf 'CODEXHUB_TEST_LAUNCHER=%s\n' "$launcher"
rm -rf "$root"
exit 0
"###
        .replace("__RECONCILE_SCRIPT__", generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "indirect launcher alias fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_runtime_reconcile_output(&ssh::SshCommandOutput {
            exit_code: Some(1),
            ..output.clone()
        })
        .expect("indirect launcher alias result");
        assert_eq!(parsed.status, CodexRuntimeReconcileStatus::ManualRequired);
        assert_eq!(parsed.reason, "target-file-identity-unknown");
        assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=1"));
        assert!(output.stdout.contains("CODEXHUB_TEST_LAUNCHER=preserved"));
    }

    #[test]
    fn reconcile_restores_exact_floor_when_current_is_missing_or_ambiguous() {
        let current_floor = StrictCurrentRuntime {
            version: "0.144.5".into(),
            release_entry: "0.144.5".into(),
            binary_relative_path: "bin/codex".into(),
        };
        let generated = remote_codex_runtime_reconcile_script_with_floors(
            Some("0.144.5"),
            Some(&current_floor),
        )
        .expect("exact current recovery script");

        for (name, current_setup) in [
            ("missing", "# installer deleted standalone/current"),
            (
                "ambiguous",
                r#"cp "$releases/0.143.0/bin/codex" "$releases/0.143.0/codex"
chmod 700 "$releases/0.143.0/codex"
ln -s "$releases/0.143.0" "$home/.codex/packages/standalone/current""#,
            ),
        ] {
            let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$releases/0.143.0/bin" "$releases/0.144.5/bin"
for version in 0.143.0 0.144.5; do
  cat >"$releases/$version/bin/codex" <<CODEXHUB_TEST_BIN
#!/bin/sh
printf 'codex-cli $version\n'
CODEXHUB_TEST_BIN
  chmod 700 "$releases/$version/bin/codex"
done
__CURRENT_SETUP__
ln -s "$home/.codex/packages/standalone/current/bin/codex" "$home/.local/bin/codex"
printf '%s\n' "$home/.codex/packages/standalone/current/bin/codex" >"$home/.codex-hub/codex-target"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
cat "$root/out"
printf 'CODEXHUB_TEST_CURRENT=%s\n' "$(readlink -f "$home/.codex/packages/standalone/current")"
printf 'CODEXHUB_TEST_VERSION=%s\n' "$(HOME="$home" "$home/.local/bin/codex" --version | awk '{ print $NF }')"
rm -rf "$root"
"###
            .replace("__CURRENT_SETUP__", current_setup)
            .replace("__RECONCILE_SCRIPT__", &generated);
            let Some(output) = run_sh(&harness) else {
                return;
            };
            assert!(
                output.success(),
                "{name} current recovery fixture failed: {}",
                output.stderr
            );
            let parsed = parse_remote_codex_runtime_reconcile_output(&output)
                .expect("exact current recovery result");
            assert_eq!(parsed.target_version.as_deref(), Some("0.144.5"));
            assert!(output.stdout.contains("/releases/0.144.5"));
            assert!(output.stdout.contains("CODEXHUB_TEST_VERSION=0.144.5"));
        }
    }

    #[test]
    fn reconcile_rejects_ambiguous_direct_target_before_managed_file_switch() {
        let generated = remote_codex_runtime_reconcile_script();
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$releases/0.143.0/bin" "$releases/0.144.5/bin"
for version in 0.143.0 0.144.5; do
  cat >"$releases/$version/bin/codex" <<CODEXHUB_TEST_BIN
#!/bin/sh
printf 'codex-cli $version\n'
CODEXHUB_TEST_BIN
  chmod 700 "$releases/$version/bin/codex"
done
cp "$releases/0.143.0/bin/codex" "$releases/0.143.0/codex"
chmod 700 "$releases/0.143.0/codex"
ln -s "$releases/0.144.5" "$home/.codex/packages/standalone/current"
ln -s "$releases/0.144.5/bin/codex" "$home/.local/bin/codex"
printf '%s\n' "$releases/0.143.0/bin/codex" >"$home/.codex-hub/codex-target"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
set +e
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
status=$?
set -e
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
printf 'CODEXHUB_TEST_TARGET=%s\n' "$(sed -n '1p' "$home/.codex-hub/codex-target")"
printf 'CODEXHUB_TEST_CURRENT=%s\n' "$(readlink -f "$home/.codex/packages/standalone/current")"
rm -rf "$root"
exit 0
"###
        .replace("__RECONCILE_SCRIPT__", generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "dual-target fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_runtime_reconcile_output(&ssh::SshCommandOutput {
            exit_code: Some(1),
            ..output.clone()
        })
        .expect("manual ambiguous-target result");
        assert_eq!(parsed.status, CodexRuntimeReconcileStatus::ManualRequired);
        assert_eq!(parsed.reason, "target-release-identity-unknown");
        assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=1"));
        assert!(output.stdout.contains("/releases/0.143.0/bin/codex"));
        assert!(output.stdout.contains("/releases/0.144.5"));
    }

    #[test]
    fn reconcile_rejects_dual_layout_hidden_by_intermediate_directory_symlink() {
        let generated = remote_codex_runtime_reconcile_script();
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$releases/0.143.0/bin" "$releases/0.144.5/bin"
for version in 0.143.0 0.144.5; do
  cat >"$releases/$version/bin/codex" <<CODEXHUB_TEST_BIN
#!/bin/sh
printf 'codex-cli $version\n'
CODEXHUB_TEST_BIN
  chmod 700 "$releases/$version/bin/codex"
done
cp "$releases/0.143.0/bin/codex" "$releases/0.143.0/codex"
chmod 700 "$releases/0.143.0/codex"
ln -s "$releases" "$home/release-alias"
ln -s "$releases/0.144.5" "$home/.codex/packages/standalone/current"
ln -s "$releases/0.144.5/bin/codex" "$home/.local/bin/codex"
printf '%s\n' "$home/release-alias/0.143.0/bin/codex" >"$home/.codex-hub/codex-target"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
set +e
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
status=$?
set -e
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
if [ -L "$home/.local/bin/codex" ]; then launcher=preserved; else launcher=changed; fi
printf 'CODEXHUB_TEST_LAUNCHER=%s\n' "$launcher"
rm -rf "$root"
exit 0
"###
        .replace("__RECONCILE_SCRIPT__", generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "intermediate symlink fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_runtime_reconcile_output(&ssh::SshCommandOutput {
            exit_code: Some(1),
            ..output.clone()
        })
        .expect("manual canonical dual-layout result");
        assert_eq!(parsed.status, CodexRuntimeReconcileStatus::ManualRequired);
        assert_eq!(parsed.reason, "target-file-identity-unknown");
        assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=1"));
        assert!(output.stdout.contains("CODEXHUB_TEST_LAUNCHER=preserved"));
    }

    #[test]
    fn reconcile_restores_exact_official_suffix_and_legacy_layout() {
        let floor_entry = "0.144.5-x86_64-unknown-linux-musl";
        let current_floor = StrictCurrentRuntime {
            version: "0.144.5".into(),
            release_entry: floor_entry.into(),
            binary_relative_path: "codex".into(),
        };
        let generated = remote_codex_runtime_reconcile_script_with_floors(
            Some("0.142.5"),
            Some(&current_floor),
        )
        .expect("official exact floor script");
        assert!(generated.contains(&format!("codexhub_minimum_current_entry='{floor_entry}'")));
        assert!(generated.contains("codexhub_minimum_current_binary_relative_path='codex'"));
        assert!(generated.contains(
            "floor_binary=\"$release_root/$minimum_current_entry/$minimum_current_binary_relative_path\""
        ));

        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
floor_entry=0.144.5-x86_64-unknown-linux-musl
stale_entry=0.143.0-x86_64-unknown-linux-musl
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$releases/$floor_entry" "$releases/$stale_entry/bin"
cat >"$releases/$floor_entry/codex" <<'CODEXHUB_FLOOR_BIN'
#!/bin/sh
printf 'codex-cli 0.144.5\n'
CODEXHUB_FLOOR_BIN
cat >"$releases/$stale_entry/bin/codex" <<'CODEXHUB_STALE_BIN'
#!/bin/sh
printf 'codex-cli 0.143.0\n'
CODEXHUB_STALE_BIN
chmod 700 "$releases/$floor_entry/codex" "$releases/$stale_entry/bin/codex"
ln -s "$releases/$stale_entry" "$home/.codex/packages/standalone/current"
ln -s "$releases/$stale_entry/bin/codex" "$home/.local/bin/codex"
printf '%s\n' "$releases/$stale_entry/bin/codex" >"$home/.codex-hub/codex-target"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
cat "$root/out"
printf 'CODEXHUB_TEST_CURRENT=%s\n' "$(readlink -f "$home/.codex/packages/standalone/current")"
printf 'CODEXHUB_TEST_TARGET=%s\n' "$(sed -n '1p' "$home/.codex-hub/codex-target")"
cat "$releases/$floor_entry/.codexhub-managed-release"
rm -rf "$root"
"###
        .replace("__RECONCILE_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "official suffix fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_runtime_reconcile_output(&output)
            .expect("official suffix reconcile result");
        assert_eq!(parsed.target_version.as_deref(), Some("0.144.5"));
        assert!(output.stdout.contains(&format!("/releases/{floor_entry}")));
        assert!(output
            .stdout
            .contains("/.codex/packages/standalone/current/codex"));
        assert!(output.stdout.contains("version=0.144.5"));
    }

    #[test]
    fn reconcile_rejects_ambiguous_current_with_one_bad_executable_layout() {
        let generated = remote_codex_runtime_reconcile_script();
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
release="$home/.codex/packages/standalone/releases/0.144.5"
mkdir -p "$home/.local/bin" "$home/.codex-hub" "$release/bin"
cat >"$release/bin/codex" <<'CODEXHUB_GOOD_BIN'
#!/bin/sh
printf 'codex-cli 0.144.5\n'
CODEXHUB_GOOD_BIN
cat >"$release/codex" <<'CODEXHUB_BAD_BIN'
#!/bin/sh
printf 'invalid-version\n'
CODEXHUB_BAD_BIN
chmod 700 "$release/bin/codex" "$release/codex"
ln -s "$release" "$home/.codex/packages/standalone/current"
ln -s "$release/bin/codex" "$home/.local/bin/codex"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
set +e
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
status=$?
set -e
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
if [ -L "$home/.local/bin/codex" ]; then launcher=preserved; else launcher=changed; fi
printf 'CODEXHUB_TEST_LAUNCHER=%s\n' "$launcher"
rm -rf "$root"
exit 0
"###
        .replace("__RECONCILE_SCRIPT__", generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "ambiguous current fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_runtime_reconcile_output(&ssh::SshCommandOutput {
            exit_code: Some(1),
            ..output.clone()
        })
        .expect("manual ambiguous-current result");
        assert_eq!(parsed.status, CodexRuntimeReconcileStatus::ManualRequired);
        assert_eq!(parsed.reason, "current-release-identity-unknown");
        assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=1"));
        assert!(output.stdout.contains("CODEXHUB_TEST_LAUNCHER=preserved"));
    }

    #[test]
    fn reconcile_capture_rollback_restores_launcher_before_removing_sidecar() {
        let generated = remote_codex_runtime_reconcile_script().replace(
            "launcher_tmp=\"$launcher.codexhub.tmp.$$\"",
            "launcher_tmp=\"$hub_dir\"",
        );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
mkdir -p "$home/.local/bin" "$home/.codex-hub"
cat >"$home/.local/bin/codex" <<'CODEXHUB_EXTERNAL_BIN'
#!/bin/sh
printf 'codex-cli 0.144.5\n'
CODEXHUB_EXTERNAL_BIN
chmod 700 "$home/.local/bin/codex"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
set +e
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out" 2>/dev/null
status=$?
set -e
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
printf 'CODEXHUB_TEST_LAUNCHER_VERSION=%s\n' "$("$home/.local/bin/codex" --version | awk '{ print $NF }')"
capture_count=0
for capture in "$home/.codex-hub"/codex-original.*; do
  [ -e "$capture" ] || [ -L "$capture" ] || continue
  capture_count=$((capture_count + 1))
done
printf 'CODEXHUB_TEST_CAPTURE_COUNT=%s\n' "$capture_count"
if [ -e "$home/.codex-hub/codex-target" ]; then target=present; else target=absent; fi
printf 'CODEXHUB_TEST_TARGET=%s\n' "$target"
rm -rf "$root"
exit 0
"###
        .replace("__RECONCILE_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "capture rollback fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_runtime_reconcile_output(&ssh::SshCommandOutput {
            exit_code: Some(1),
            ..output.clone()
        })
        .expect("manual capture rollback result");
        assert_eq!(parsed.status, CodexRuntimeReconcileStatus::ManualRequired);
        assert_eq!(parsed.reason, "launcher-stage-failed");
        assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=1"));
        assert!(output
            .stdout
            .contains("CODEXHUB_TEST_LAUNCHER_VERSION=0.144.5"));
        assert!(output.stdout.contains("CODEXHUB_TEST_CAPTURE_COUNT=0"));
        assert!(output.stdout.contains("CODEXHUB_TEST_TARGET=absent"));
    }

    #[test]
    fn reconcile_signal_paths_roll_back_each_completed_mutation_before_exit_unlock() {
        let script = remote_codex_runtime_reconcile_script();
        for (label, mutation) in [
            ("launcher capture", "mv \"$launcher\" \"$capture_path\""),
            ("target replace", "mv \"$target_tmp\" \"$target_file\""),
            ("launcher replace", "mv \"$launcher_tmp\" \"$launcher\""),
        ] {
            let injected = format!("{mutation}; runtime_signal_failure");
            let generated = script.replacen(mutation, &injected, 1);
            assert!(
                generated.contains(&injected),
                "missing injected signal point after {label}"
            );
            assert!(generated.contains("trap runtime_signal_failure HUP INT TERM"));
            let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
mkdir -p "$home/.local/bin" "$home/.codex-hub"
cat >"$home/.local/bin/codex" <<'CODEXHUB_EXTERNAL_BIN'
#!/bin/sh
printf 'codex-cli 0.144.5\n'
CODEXHUB_EXTERNAL_BIN
chmod 700 "$home/.local/bin/codex"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
set +e
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/out"
status=$?
set -e
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
printf 'CODEXHUB_TEST_LAUNCHER_VERSION=%s\n' "$("$home/.local/bin/codex" --version | awk '{ print $NF }')"
if [ -e "$home/.codex-hub/codex-target" ]; then target=present; else target=absent; fi
if [ -e "$home/.codexhub-runtime-cleanup.lock" ]; then lock=present; else lock=absent; fi
printf 'CODEXHUB_TEST_TARGET=%s\n' "$target"
printf 'CODEXHUB_TEST_LOCK=%s\n' "$lock"
rm -rf "$root"
exit 0
"###
            .replace("__RECONCILE_SCRIPT__", &generated);
            let Some(output) = run_sh(&harness) else {
                return;
            };
            assert!(
                output.success(),
                "signal rollback fixture failed after {label}: {}",
                output.stderr
            );
            let parsed = parse_remote_codex_runtime_reconcile_output(&ssh::SshCommandOutput {
                exit_code: Some(1),
                ..output.clone()
            })
            .expect("signal rollback runtime result");
            assert_eq!(parsed.status, CodexRuntimeReconcileStatus::ManualRequired);
            assert_eq!(parsed.reason, "runtime-interrupted");
            assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=1"), "{label}");
            assert!(
                output
                    .stdout
                    .contains("CODEXHUB_TEST_LAUNCHER_VERSION=0.144.5"),
                "{label}"
            );
            assert!(
                output.stdout.contains("CODEXHUB_TEST_TARGET=absent"),
                "{label}"
            );
            assert!(
                output.stdout.contains("CODEXHUB_TEST_LOCK=absent"),
                "{label}"
            );
        }
    }

    #[test]
    fn cleanup_script_has_strict_identity_second_check_and_no_process_signals() {
        let script = remote_codex_release_cleanup_script();
        for required in [
            "release_root=\"$standalone_root/releases\"",
            "marker_name=\".codexhub-managed-release\"",
            "[ -f \"$marker\" ] && [ ! -L \"$marker\" ]",
            "[ \"$parent_real\" = \"$release_root_real\" ]",
            "for candidate in \"$release_root\"/*",
            "for proc_dir in \"$proc_root\"/[0-9]*",
            "if ! verify_candidate \"$candidate\" ||",
            "quarantine=\"$release_root/.codexhub-quarantine.$candidate_name.$$\"",
            "mv -T -n \"$candidate\" \"$quarantine\"",
            "verify_quarantine \"$quarantine\" \"$candidate_name\"",
            "active_candidate=\"\"",
            "rm -rf \"$quarantine\"",
            "binary_version=$(normalized_version_for_binary",
            "[ \"$release_binary_match_count\" -eq 1 ] || return 1",
            "capture_marker_suffix=\".codexhub-managed-capture\"",
            "verify_capture_candidate \"$candidate\"",
            "[ \"$observed_proc_exe\" = \"$capture_real\" ]",
            "quarantine=\"$hub_dir/.codexhub-capture-quarantine.$candidate_name.$$\"",
            "verify_capture_quarantine \"$quarantine\" \"$candidate_name\"",
            "recover_orphaned_capture_quarantines",
            "rm -f \"$capture_marker\"",
            "cleanup_lock=\"$cleanup_lock_root/.codexhub-runtime-cleanup.lock\"",
            "CodexHub runtime cleanup lock v1",
            "cleanup_lock_owner_activity",
            "verify_owned_cleanup_lock",
            "cleanup-lock-active",
            "read_current_uid_process_identity",
            "read_cleanup_process_uid_starttime",
            "proc_starttime_after",
            "proc_exe_after",
            "release_in_use_now \"$candidate_real\" \"$isolated_real\"",
            "capture_in_use_now \"$candidate_real\" \"$isolated_real\"",
            "trap 'trap - EXIT; cleanup_work_dir; exit 143' TERM",
            "backup_root=\"$hub_dir/deletion-backups\"",
            "ensure_update_backup",
            "backup_destination=\"$backup_releases/$candidate_name\"",
            "mv -T -n \"$quarantine\" \"$backup_destination\"",
            "backup_residual_links_for_release",
            "CODEXHUB_CLEANUP_BACKED_UP",
            "CODEXHUB_CLEANUP_BACKUP_ID",
            "CODEXHUB_CLEANUP_IGNORED_SESSION_PROCESSES",
        ] {
            assert!(
                script.contains(required),
                "missing cleanup guard: {required}"
            );
        }
        assert!(script.matches("verify_candidate \"$candidate\"").count() >= 2);
        assert!(
            script
                .matches("verify_capture_quarantine \"$quarantine\"")
                .count()
                >= 2
        );
        let capture_binary_remove = script
            .find("if ! rm -f \"$quarantine\"; then")
            .expect("capture binary removal");
        let capture_marker_remove = script
            .find("rm -f \"$capture_marker\"; then")
            .expect("capture marker removal");
        assert!(capture_binary_remove < capture_marker_remove);
        let lock_acquire = script
            .find("acquire_cleanup_lock\ncleanup_lock_status=$?")
            .expect("cleanup lock acquisition");
        let quarantine_recovery = script
            .find("recover_orphaned_quarantines ||")
            .expect("release quarantine recovery");
        let release_remove = script
            .find("rm -rf \"$quarantine\"")
            .expect("release quarantine removal");
        let final_lock_check = script[..release_remove]
            .rfind("verify_owned_cleanup_lock")
            .expect("final release lock check");
        assert!(lock_acquire < quarantine_recovery);
        assert!(quarantine_recovery < final_lock_check && final_lock_check < release_remove);
        assert!(script.matches("mv -T -n").count() >= 6);
        assert!(!script.contains("trap cleanup_work_dir EXIT HUP INT TERM"));
        for forbidden in [
            "find ",
            "pkill",
            "killall",
            "kill -9",
            "SIGKILL",
            "kill -TERM",
            "rm -rf \"$release_root",
            "rm -rf $candidate",
            "rm -rf \"$candidate\"",
        ] {
            assert!(
                !script.contains(forbidden),
                "found forbidden token {forbidden}"
            );
        }
    }

    #[test]
    fn update_cleanup_policy_enables_staged_backup_only_for_verified_updates() {
        let update = remote_codex_release_cleanup_script_with_policy(
            &CodexReleaseCleanupPolicy::VerifiedOlderThan("codex-cli 0.145.0".into()),
        )
        .expect("verified update cleanup script");
        let managed = remote_codex_release_cleanup_script_with_policy(
            &CodexReleaseCleanupPolicy::ManagedOnly,
        )
        .expect("managed cleanup script");

        assert!(update.starts_with(
            "codexhub_cleanup_policy='verified-older-than'\ncodexhub_cleanup_verified_version='0.145.0'\ncodexhub_cleanup_backup_mode='staged'\n"
        ));
        assert!(managed.starts_with(
            "codexhub_cleanup_policy='managed-only'\ncodexhub_cleanup_verified_version=''\ncodexhub_cleanup_backup_mode='none'\n"
        ));
        assert!(update.contains("backup_root=\"$hub_dir/deletion-backups\""));
        assert!(update.contains("backed_up=$((backed_up + 1))"));
        assert!(!managed.starts_with("codexhub_cleanup_backup_mode='staged'"));
    }

    #[test]
    fn release_cleanup_uses_a_dedicated_multi_candidate_ssh_budget() {
        assert_eq!(REMOTE_CODEX_RELEASE_CLEANUP_TIMEOUT_MS, 360_000);
        let source = include_str!("codex_runtime.rs");
        assert!(source.contains(
            "ssh::run_ssh_script_with_extended_timeout(alias, &script, cleanup_timeout)"
        ));
    }

    #[test]
    fn staged_update_ignores_only_stable_classified_session_processes() {
        for (label, comm, arg0) in [
            ("sshd", "sshd", "sshd: codex@pts/0"),
            ("sd-pam", "(sd-pam)", "(sd-pam)"),
            ("sftp-server", "sftp-server", "/usr/lib/openssh/sftp-server"),
            ("fusermount3", "fusermount3", "/usr/bin/fusermount3"),
        ] {
            let Some(output) = run_session_cleanup_fixture(
                CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
                comm,
                arg0,
                ":",
            ) else {
                return;
            };
            assert!(
                output.success(),
                "{label} fixture failed: {}",
                output.stderr
            );
            let parsed = parse_remote_codex_release_cleanup_output(&output)
                .unwrap_or_else(|error| panic!("{label} cleanup result: {error}"));
            assert_eq!(
                parsed.status,
                CodexReleaseCleanupStatus::Completed,
                "{label}"
            );
            assert_eq!(parsed.ignored_session_process_count, 1, "{label}");
            assert_eq!(parsed.backed_up_count, 1, "{label}");
            assert!(
                parsed.safe_summary().contains("ignoredSessionProcesses=1"),
                "{label}"
            );
            assert!(output.stdout.contains("CODEXHUB_TEST_OLD_ACTIVE=absent"));
            assert!(output.stdout.contains("CODEXHUB_TEST_OLD_BACKUP=present"));
        }
    }

    #[test]
    fn staged_update_accepts_only_exact_systemd_user_manager_identity() {
        for path in ["/usr/lib/systemd/systemd", "/lib/systemd/systemd"] {
            let Some(output) = run_staged_process_cleanup_fixture(
                CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
                "systemd",
                "S",
                &[path, "--user"],
                false,
                ":",
            ) else {
                return;
            };
            assert!(
                output.success(),
                "systemd fixture failed: {}",
                output.stderr
            );
            let parsed = parse_remote_codex_release_cleanup_output(&output)
                .expect("exact systemd user cleanup result");
            assert_eq!(parsed.status, CodexReleaseCleanupStatus::Completed);
            assert_eq!(parsed.ignored_session_process_count, 1);
            assert_eq!(parsed.backed_up_count, 1);
            assert!(output.stdout.contains("CODEXHUB_TEST_OLD_ACTIVE=absent"));
            assert!(output.stdout.contains("CODEXHUB_TEST_OLD_BACKUP=present"));
        }

        let invalid = [
            (
                "missing-user-mode",
                "systemd",
                "S",
                vec!["/usr/lib/systemd/systemd"],
                ":",
            ),
            (
                "extra-systemd-argument",
                "systemd",
                "S",
                vec!["/usr/lib/systemd/systemd", "--user", "--deserialize"],
                ":",
            ),
            (
                "relative-systemd-path",
                "systemd",
                "S",
                vec!["systemd", "--user"],
                ":",
            ),
            (
                "wrong-systemd-comm",
                "systemd-user",
                "S",
                vec!["/usr/lib/systemd/systemd", "--user"],
                ":",
            ),
            (
                "systemd-cmdline-changed",
                "systemd",
                "S",
                vec!["/usr/lib/systemd/systemd", "--user"],
                "printf '%s\\000' '/usr/lib/systemd/systemd' '--user' '--deserialize' >\"$proc_root/610/cmdline\"",
            ),
        ];
        for (label, comm, state, argv, mutation) in invalid {
            let Some(output) = run_staged_process_cleanup_fixture(
                CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
                comm,
                state,
                &argv,
                false,
                mutation,
            ) else {
                return;
            };
            assert!(
                output.success(),
                "{label} fixture failed: {}",
                output.stderr
            );
            let parsed = parse_remote_codex_release_cleanup_output(&output)
                .unwrap_or_else(|error| panic!("{label} cleanup result: {error}"));
            assert_eq!(
                parsed.status,
                CodexReleaseCleanupStatus::Deferred,
                "{label}"
            );
            assert_eq!(parsed.reason, "proc-identity-unknown", "{label}");
            assert_eq!(parsed.backed_up_count, 0, "{label}");
            assert!(output.stdout.contains("CODEXHUB_TEST_OLD_ACTIVE=present"));
        }

        let Some(managed) = run_staged_process_cleanup_fixture(
            CodexReleaseCleanupPolicy::ManagedOnly,
            "systemd",
            "S",
            &["/usr/lib/systemd/systemd", "--user"],
            false,
            ":",
        ) else {
            return;
        };
        let parsed = parse_remote_codex_release_cleanup_output(&managed)
            .expect("managed systemd cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Deferred);
        assert_eq!(parsed.ignored_session_process_count, 0);
        assert_eq!(parsed.backed_up_count, 0);
    }

    #[test]
    fn staged_update_accepts_only_stable_single_thread_zombies() {
        let Some(output) = run_staged_process_cleanup_fixture(
            CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
            "git",
            "Z",
            &[],
            false,
            ":",
        ) else {
            return;
        };
        assert!(output.success(), "zombie fixture failed: {}", output.stderr);
        let parsed = parse_remote_codex_release_cleanup_output(&output)
            .expect("stable zombie cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Completed);
        assert_eq!(parsed.ignored_session_process_count, 1);
        assert_eq!(parsed.backed_up_count, 1);

        let invalid = [
            (
                "live-empty-cmdline",
                "S",
                Vec::<&str>::new(),
                false,
                ":",
            ),
            ("zombie-nonempty-cmdline", "Z", vec!["git"], false, ":"),
            ("zombie-sibling-thread", "Z", vec![], true, ":"),
            (
                "zombie-new-sibling-thread",
                "Z",
                vec![],
                false,
                "mkdir -p \"$proc_root/610/task/611\"",
            ),
            (
                "zombie-state-changed",
                "Z",
                vec![],
                false,
                "printf '610 (session) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1001\\n' >\"$proc_root/610/stat\"",
            ),
            (
                "zombie-thread-count-changed",
                "Z",
                vec![],
                false,
                "printf 'Uid:\\t%s\\t%s\\t%s\\t%s\\nThreads:\\t2\\n' \"$current_uid\" \"$current_uid\" \"$current_uid\" \"$current_uid\" >\"$proc_root/610/status\"",
            ),
            (
                "zombie-exe-became-readable",
                "Z",
                vec![],
                false,
                "rm -f \"$proc_root/610/codexhub-test-unreadable-exe\"\nln -s \"$release_root/0.142.5/bin/codex\" \"$proc_root/610/exe\"",
            ),
        ];
        for (label, state, argv, extra_task, mutation) in invalid {
            let Some(output) = run_staged_process_cleanup_fixture(
                CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
                "git",
                state,
                &argv,
                extra_task,
                mutation,
            ) else {
                return;
            };
            assert!(
                output.success(),
                "{label} fixture failed: {}",
                output.stderr
            );
            let parsed = parse_remote_codex_release_cleanup_output(&output)
                .unwrap_or_else(|error| panic!("{label} cleanup result: {error}"));
            assert_eq!(
                parsed.status,
                CodexReleaseCleanupStatus::Deferred,
                "{label}"
            );
            assert_eq!(parsed.reason, "proc-identity-unknown", "{label}");
            assert_eq!(parsed.backed_up_count, 0, "{label}");
        }

        let task_handoff = r#"case "$zombie_task_id" in
  611)
    rm -rf "$zombie_task_root/611"
    mkdir -p "$zombie_task_root/612"
    ;;
  612)
    rm -rf "$zombie_task_root/612"
    mkdir -p "$zombie_task_root/611"
    ;;
esac"#;
        let Some(handoff) = run_staged_process_cleanup_fixture_with_task_hook(
            CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
            "git",
            "Z",
            &[],
            true,
            ":",
            task_handoff,
        ) else {
            return;
        };
        let parsed = parse_remote_codex_release_cleanup_output(&handoff)
            .expect("alternating zombie task cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Deferred);
        assert_eq!(parsed.reason, "proc-identity-unknown");
        assert_eq!(parsed.ignored_session_process_count, 0);
        assert_eq!(parsed.backed_up_count, 0);

        let Some(managed) = run_staged_process_cleanup_fixture(
            CodexReleaseCleanupPolicy::ManagedOnly,
            "git",
            "Z",
            &[],
            false,
            ":",
        ) else {
            return;
        };
        let parsed = parse_remote_codex_release_cleanup_output(&managed)
            .expect("managed zombie cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Deferred);
        assert_eq!(parsed.ignored_session_process_count, 0);

        let Some(reaped) = run_staged_process_cleanup_fixture(
            CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
            "git",
            "Z",
            &[],
            false,
            "rm -rf \"$proc_root/610\"",
        ) else {
            return;
        };
        let parsed = parse_remote_codex_release_cleanup_output(&reaped)
            .expect("reaped zombie cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Completed);
        assert_eq!(parsed.ignored_session_process_count, 1);
        assert_eq!(parsed.backed_up_count, 1);
    }

    #[test]
    fn managed_cleanup_and_unknown_sessions_keep_unreadable_executables_strict() {
        for (label, policy, comm, arg0) in [
            (
                "managed-sshd",
                CodexReleaseCleanupPolicy::ManagedOnly,
                "sshd",
                "sshd: codex@pts/0",
            ),
            (
                "unknown-comm",
                CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
                "codex-helper",
                "sshd: codex@pts/0",
            ),
            (
                "empty-sshd-session",
                CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
                "sshd",
                "sshd: ",
            ),
            (
                "near-sftp-name",
                CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
                "sftp-server",
                "/usr/lib/openssh/sftp-server-helper",
            ),
        ] {
            let Some(output) = run_session_cleanup_fixture(policy, comm, arg0, ":") else {
                return;
            };
            assert!(
                output.success(),
                "{label} fixture failed: {}",
                output.stderr
            );
            let parsed = parse_remote_codex_release_cleanup_output(&output)
                .unwrap_or_else(|error| panic!("{label} cleanup result: {error}"));
            assert_eq!(
                parsed.status,
                CodexReleaseCleanupStatus::Deferred,
                "{label}"
            );
            assert_eq!(parsed.reason, "proc-identity-unknown", "{label}");
            assert_eq!(parsed.ignored_session_process_count, 0, "{label}");
            assert_eq!(parsed.backed_up_count, 0, "{label}");
            assert!(output.stdout.contains("CODEXHUB_TEST_OLD_ACTIVE=present"));
            assert!(output.stdout.contains("CODEXHUB_TEST_OLD_BACKUP=absent"));
        }
    }

    #[test]
    fn staged_session_snapshot_rejects_identity_changes_and_new_processes() {
        let mutations = [
            (
                "argv0-change",
                "printf 'sshd: changed@pts/1\\000' >\"$proc_root/610/cmdline\"",
            ),
            (
                "starttime-change",
                "printf '610 (session) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 2002\\n' >\"$proc_root/610/stat\"",
            ),
            (
                "comm-shape-change",
                "printf 'sshd\\nextra\\n' >\"$proc_root/610/comm\"",
            ),
            (
                "new-session",
                "mkdir -p \"$proc_root/611\"\nprintf 'Uid:\\t%s\\t%s\\t%s\\t%s\\n' \"$current_uid\" \"$current_uid\" \"$current_uid\" \"$current_uid\" >\"$proc_root/611/status\"\nprintf '611 (session) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1002\\n' >\"$proc_root/611/stat\"\nprintf 'sshd\\n' >\"$proc_root/611/comm\"\nprintf 'sshd: new@pts/2\\000' >\"$proc_root/611/cmdline\"\n: >\"$proc_root/611/codexhub-test-unreadable-exe\"",
            ),
            (
                "exe-became-readable",
                "rm -f \"$proc_root/610/codexhub-test-unreadable-exe\"\nln -s \"$release_root/0.142.5/bin/codex\" \"$proc_root/610/exe\"",
            ),
        ];
        for (label, mutation) in mutations {
            let Some(output) = run_session_cleanup_fixture(
                CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
                "sshd",
                "sshd: codex@pts/0",
                mutation,
            ) else {
                return;
            };
            assert!(
                output.success(),
                "{label} fixture failed: {}",
                output.stderr
            );
            let parsed = parse_remote_codex_release_cleanup_output(&output)
                .unwrap_or_else(|error| panic!("{label} cleanup result: {error}"));
            assert_eq!(
                parsed.status,
                CodexReleaseCleanupStatus::Deferred,
                "{label}"
            );
            assert_eq!(parsed.reason, "proc-identity-unknown", "{label}");
            assert_eq!(parsed.ignored_session_process_count, 1, "{label}");
            assert_eq!(parsed.backed_up_count, 0, "{label}");
            assert!(output.stdout.contains("CODEXHUB_TEST_OLD_ACTIVE=present"));
            assert!(output.stdout.contains("CODEXHUB_TEST_OLD_BACKUP=absent"));
        }
    }

    #[test]
    fn staged_session_that_exits_after_snapshot_does_not_block_backup() {
        let Some(output) = run_session_cleanup_fixture(
            CodexReleaseCleanupPolicy::VerifiedOlderThan("0.145.0".into()),
            "sshd",
            "sshd: codex@pts/0",
            "rm -rf \"$proc_root/610\"",
        ) else {
            return;
        };
        assert!(output.success(), "exit fixture failed: {}", output.stderr);
        let parsed = parse_remote_codex_release_cleanup_output(&output)
            .expect("exited session cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Completed);
        assert_eq!(parsed.ignored_session_process_count, 1);
        assert_eq!(parsed.backed_up_count, 1);
        assert!(output.stdout.contains("CODEXHUB_TEST_OLD_ACTIVE=absent"));
        assert!(output.stdout.contains("CODEXHUB_TEST_OLD_BACKUP=present"));
    }

    #[test]
    fn concurrent_cleanup_defers_active_owner_without_recovering_its_quarantine() {
        let base = remote_codex_release_cleanup_script()
            .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        let first = base.replace(
            "recover_orphaned_quarantines || defer_all_direct_entries quarantine-recovery-unsafe",
            ": >\"$HOME/cleanup-lock-acquired\"\nsleep 2\nrecover_orphaned_quarantines || defer_all_direct_entries quarantine-recovery-unsafe",
        );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
hub="$home/.codex-hub"
releases="$home/.codex/packages/standalone/releases"
release="$releases/0.142.5"
capture_name=codex-original.20260101000000.401
quarantine="$hub/.codexhub-capture-quarantine.$capture_name.777"
mkdir -p "$release/bin" "$hub" "$home/proc"
cat >"$release/bin/codex" <<'CODEXHUB_RELEASE_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_RELEASE_BIN
cat >"$quarantine" <<'CODEXHUB_CAPTURE_BIN'
#!/bin/sh
printf 'codex-cli 0.141.0\n'
CODEXHUB_CAPTURE_BIN
chmod 700 "$release/bin/codex" "$quarantine"
printf 'CodexHub managed standalone release v1\nversion=0.142.5\n' >"$release/.codexhub-managed-release"
printf 'CodexHub managed launcher capture v1\nname=%s\nversion=0.141.0\n' "$capture_name" >"$hub/$capture_name.codexhub-managed-capture"
cat >"$root/first.sh" <<'CODEXHUB_FIRST_SCRIPT'
__FIRST_SCRIPT__
CODEXHUB_FIRST_SCRIPT
cat >"$root/second.sh" <<'CODEXHUB_SECOND_SCRIPT'
__SECOND_SCRIPT__
CODEXHUB_SECOND_SCRIPT
HOME="$home" sh "$root/first.sh" >"$root/first.out" &
first_pid=$!
while [ ! -f "$home/cleanup-lock-acquired" ]; do sleep 0.05; done
HOME="$home" sh "$root/second.sh" >"$root/second.out"
if [ -e "$hub/$capture_name" ]; then concurrent_capture=recovered; else concurrent_capture=absent; fi
if [ -e "$quarantine" ]; then concurrent_quarantine=present; else concurrent_quarantine=absent; fi
wait "$first_pid"
printf 'CODEXHUB_TEST_SECOND\n'
cat "$root/second.out"
printf 'CODEXHUB_TEST_CONCURRENT_CAPTURE=%s\n' "$concurrent_capture"
printf 'CODEXHUB_TEST_CONCURRENT_QUARANTINE=%s\n' "$concurrent_quarantine"
printf 'CODEXHUB_TEST_FIRST\n'
cat "$root/first.out"
rm -rf "$root"
"###
        .replace("__FIRST_SCRIPT__", &first)
        .replace("__SECOND_SCRIPT__", &base);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "concurrent cleanup fixture failed: {}",
            output.stderr
        );
        let second = output
            .stdout
            .split_once("CODEXHUB_TEST_SECOND\n")
            .and_then(|(_, tail)| {
                tail.split_once("CODEXHUB_TEST_FIRST\n")
                    .map(|(body, _)| body)
            })
            .expect("concurrent second cleanup output");
        let parsed = parse_remote_codex_release_cleanup_output(&ssh::SshCommandOutput {
            stdout: second.into(),
            ..output.clone()
        })
        .expect("active cleanup lock result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Deferred);
        assert_eq!(parsed.reason, "cleanup-lock-active");
        assert!(second.contains("CODEXHUB_TEST_CONCURRENT_CAPTURE=absent"));
        assert!(second.contains("CODEXHUB_TEST_CONCURRENT_QUARANTINE=present"));
    }

    #[test]
    fn shared_writer_lock_survives_hub_removal_and_releases_on_exit() {
        let generated = with_remote_codex_runtime_writer_lock(
            r#"mkdir -p "$HOME/.codex-hub"
rm -rf "$HOME/.codex-hub"
if [ -f "$HOME/.codexhub-runtime-cleanup.lock" ]; then
  printf 'CODEXHUB_TEST_DURING=present\n'
else
  printf 'CODEXHUB_TEST_DURING=absent\n'
  exit 9
fi"#,
        );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
mkdir -p "$home"
cat >"$root/writer.sh" <<'CODEXHUB_WRITER_SCRIPT'
__WRITER_SCRIPT__
CODEXHUB_WRITER_SCRIPT
HOME="$home" sh "$root/writer.sh" >"$root/out"
cat "$root/out"
if [ -e "$home/.codexhub-runtime-cleanup.lock" ]; then lock_after=present; else lock_after=absent; fi
printf 'CODEXHUB_TEST_AFTER=%s\n' "$lock_after"
rm -rf "$root"
"###
        .replace("__WRITER_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "hub-removal lock fixture failed: {}",
            output.stderr
        );
        assert!(output.stdout.contains("CODEXHUB_TEST_DURING=present"));
        assert!(output.stdout.contains("CODEXHUB_TEST_AFTER=absent"));
    }

    #[test]
    fn shared_writer_fails_before_body_when_gnu_mv_no_replace_is_unavailable() {
        let generated = with_remote_codex_runtime_writer_lock(
            "printf 'CODEXHUB_TEST_BODY=ran\\n' >\"$HOME/body-ran\"",
        )
        .replace(
            "mv -T -n \"$mv_probe_source\" \"$mv_probe_destination\" >/dev/null 2>&1",
            "false",
        )
        .replace(
            "mv -T -n \"$mv_probe_move_source\" \"$mv_probe_move_destination\" >/dev/null 2>&1",
            "false",
        );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
mkdir -p "$home"
cat >"$root/writer.sh" <<'CODEXHUB_WRITER_SCRIPT'
__WRITER_SCRIPT__
CODEXHUB_WRITER_SCRIPT
set +e
HOME="$home" sh "$root/writer.sh" >"$root/out" 2>"$root/err"
status=$?
set -e
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
if [ -e "$home/body-ran" ]; then body=ran; else body=skipped; fi
if [ -e "$home/.codexhub-runtime-cleanup.lock" ]; then lock_after=present; else lock_after=absent; fi
printf 'CODEXHUB_TEST_BODY=%s\n' "$body"
printf 'CODEXHUB_TEST_LOCK=%s\n' "$lock_after"
rm -rf "$root"
"###
        .replace("__WRITER_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "unsupported writer mv fixture failed: {}",
            output.stderr
        );
        assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=75"));
        assert!(output.stdout.contains("CODEXHUB_TEST_BODY=skipped"));
        assert!(output.stdout.contains("CODEXHUB_TEST_LOCK=absent"));
    }

    #[test]
    fn shared_writer_accepts_coreutils_9_4_no_clobber_collision_status() {
        let collision_probe = "mv -T -n \"$mv_probe_source\" \"$mv_probe_destination\" >/dev/null 2>&1\n  mv_probe_status=$?";
        let mut generated =
            with_remote_codex_runtime_writer_lock("printf 'ran\\n' >\"$HOME/body-ran\"");
        assert_eq!(generated.matches(collision_probe).count(), 1);
        generated = generated.replacen(
            collision_probe,
            "mv -T -n \"$mv_probe_source\" \"$mv_probe_destination\" >/dev/null 2>&1\n  mv_probe_status=$?\n  mv_probe_status=1",
            1,
        );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
mkdir -p "$home"
cat >"$root/writer.sh" <<'CODEXHUB_WRITER_SCRIPT'
__WRITER_SCRIPT__
CODEXHUB_WRITER_SCRIPT
HOME="$home" sh "$root/writer.sh"
if [ "$(sed -n '1p' "$home/body-ran" 2>/dev/null)" = ran ]; then body=ran; else body=skipped; fi
if [ -e "$home/.codexhub-runtime-cleanup.lock" ]; then lock_after=present; else lock_after=absent; fi
printf 'CODEXHUB_TEST_BODY=%s\n' "$body"
printf 'CODEXHUB_TEST_LOCK=%s\n' "$lock_after"
rm -rf "$root"
"###
        .replace("__WRITER_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "coreutils 9.4 writer fixture failed: {}",
            output.stderr
        );
        assert!(output.stdout.contains("CODEXHUB_TEST_BODY=ran"));
        assert!(output.stdout.contains("CODEXHUB_TEST_LOCK=absent"));
    }

    #[test]
    fn shared_writer_uses_locked_runtime_floor_and_restores_exact_current_on_failure() {
        let floor_prelude = remote_version_floor_prelude(None, None).expect("empty host floors");
        let reject_body = r#"if codexhub_version_meets_floors 0.144.5; then
  : >"$HOME/mutation-ran"
  exit 9
fi
exit 69"#;
        let reject_script = format!(
            "{floor_prelude}\n{}",
            with_remote_codex_runtime_writer_lock(reject_body)
        );
        let restore_script = with_remote_codex_runtime_writer_lock(
            r#"ln -sfn "$HOME/.codex/packages/standalone/releases/0.144.5" "$HOME/.codex/packages/standalone/current"
exit 9"#,
        );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
mkdir -p "$releases/0.144.5/bin" "$releases/0.145.0/bin"
for version in 0.144.5 0.145.0; do
  cat >"$releases/$version/bin/codex" <<CODEXHUB_TEST_BIN
#!/bin/sh
printf 'codex-cli $version\n'
CODEXHUB_TEST_BIN
  chmod 700 "$releases/$version/bin/codex"
done
ln -s "$releases/0.145.0" "$home/.codex/packages/standalone/current"
cat >"$root/reject.sh" <<'CODEXHUB_REJECT_SCRIPT'
__REJECT_SCRIPT__
CODEXHUB_REJECT_SCRIPT
cat >"$root/restore.sh" <<'CODEXHUB_RESTORE_SCRIPT'
__RESTORE_SCRIPT__
CODEXHUB_RESTORE_SCRIPT
set +e
HOME="$home" PATH="$PATH" sh "$root/reject.sh"
reject_status=$?
HOME="$home" PATH="$PATH" sh "$root/restore.sh"
restore_status=$?
set -e
printf 'CODEXHUB_TEST_REJECT_STATUS=%s\n' "$reject_status"
printf 'CODEXHUB_TEST_RESTORE_STATUS=%s\n' "$restore_status"
if [ -e "$home/mutation-ran" ]; then mutation=ran; else mutation=skipped; fi
printf 'CODEXHUB_TEST_MUTATION=%s\n' "$mutation"
printf 'CODEXHUB_TEST_CURRENT=%s\n' "$(readlink -f "$home/.codex/packages/standalone/current")"
rm -rf "$root"
"###
        .replace("__REJECT_SCRIPT__", &reject_script)
        .replace("__RESTORE_SCRIPT__", &restore_script);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "locked floor fixture failed: {}",
            output.stderr
        );
        assert!(output.stdout.contains("CODEXHUB_TEST_REJECT_STATUS=69"));
        assert!(output.stdout.contains("CODEXHUB_TEST_RESTORE_STATUS=9"));
        assert!(output.stdout.contains("CODEXHUB_TEST_MUTATION=skipped"));
        assert!(output.stdout.contains("/releases/0.145.0"));
    }

    #[test]
    fn cleanup_reclaims_only_confirmed_stale_lock_and_defers_unknown_identity() {
        let generated = remote_codex_release_cleanup_script()
            .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        for (mode, owner_uid, expected_status, expected_reason, expected_release) in [
            (
                "stale",
                "$(id -u)",
                CodexReleaseCleanupStatus::Completed,
                "cleanup-complete",
                "absent",
            ),
            (
                "unknown",
                "999999999",
                CodexReleaseCleanupStatus::Deferred,
                "cleanup-lock-identity-unknown",
                "present",
            ),
        ] {
            let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
hub="$home/.codex-hub"
release="$home/.codex/packages/standalone/releases/0.142.5"
mkdir -p "$release/bin" "$hub" "$home/proc"
cat >"$release/bin/codex" <<'CODEXHUB_RELEASE_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_RELEASE_BIN
chmod 700 "$release/bin/codex"
printf 'CodexHub managed standalone release v1\nversion=0.142.5\n' >"$release/.codexhub-managed-release"
cat >"$home/.codexhub-runtime-cleanup.lock" <<CODEXHUB_LOCK
CodexHub runtime cleanup lock v1
uid=__OWNER_UID__
pid=999999999
starttime=1
CODEXHUB_LOCK
chmod 600 "$home/.codexhub-runtime-cleanup.lock"
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
HOME="$home" sh "$root/cleanup.sh" >"$root/out"
cat "$root/out"
if [ -d "$release" ]; then release_state=present; else release_state=absent; fi
if [ -e "$home/.codexhub-runtime-cleanup.lock" ]; then lock_state=present; else lock_state=absent; fi
printf 'CODEXHUB_TEST_RELEASE=%s\n' "$release_state"
printf 'CODEXHUB_TEST_LOCK=%s\n' "$lock_state"
rm -rf "$root"
"###
            .replace("__OWNER_UID__", owner_uid)
            .replace("__CLEANUP_SCRIPT__", &generated);
            let Some(output) = run_sh(&harness) else {
                return;
            };
            assert!(
                output.success(),
                "{mode} lock fixture failed: {}",
                output.stderr
            );
            let parsed = parse_remote_codex_release_cleanup_output(&output)
                .expect("stale or unknown cleanup lock result");
            assert_eq!(parsed.status, expected_status, "{mode}");
            assert_eq!(parsed.reason, expected_reason, "{mode}");
            assert!(
                output
                    .stdout
                    .contains(&format!("CODEXHUB_TEST_RELEASE={expected_release}")),
                "{mode}"
            );
            let expected_lock = if mode == "stale" { "absent" } else { "present" };
            assert!(output
                .stdout
                .contains(&format!("CODEXHUB_TEST_LOCK={expected_lock}")));
        }
    }

    #[test]
    fn cleanup_behavior_removes_only_marked_obsolete_release_and_is_idempotent() {
        let generated = remote_codex_release_cleanup_script_with_policy(
            &CodexReleaseCleanupPolicy::ManagedOnly,
        )
        .expect("managed-only cleanup script")
        .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
mkdir -p "$releases" "$home/.codex-hub" "$home/proc"
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
make_release() {
  entry=$1
  version=$2
  binary_relative_path=$3
  mkdir -p "$releases/$entry"
  binary_parent=${binary_relative_path%/*}
  if [ "$binary_parent" != "$binary_relative_path" ]; then mkdir -p "$releases/$entry/$binary_parent"; fi
  cat >"$releases/$entry/$binary_relative_path" <<CODEXHUB_TEST_BIN
#!/bin/sh
printf 'codex-cli $version\n'
CODEXHUB_TEST_BIN
  chmod 700 "$releases/$entry/$binary_relative_path"
  printf 'CodexHub managed standalone release v1\nversion=%s\n' "$version" >"$releases/$entry/.codexhub-managed-release"
}
for version in 0.139.0 0.142.5 0.143.0 0.144.5; do make_release "$version" "$version" bin/codex; done
make_release 0.138.0-x86_64-unknown-linux-musl 0.138.0 codex
make_release 0.137.0bad-x86_64-unknown-linux-musl 0.137.0 bin/codex
make_release 0.136.0 0.136.0 bin/codex
make_release 0.135.0 0.135.0 bin/codex
rm -f "$releases/0.135.0/.codexhub-managed-release"
cp "$releases/0.136.0/bin/codex" "$releases/0.136.0/codex"
chmod 700 "$releases/0.136.0/codex"
mkdir -p "$releases/0.141.0/bin" "$releases/unsafe_name/bin" "$root/outside"
: >"$releases/0.141.0/bin/codex"
: >"$releases/unsafe_name/bin/codex"
ln -s "$root/outside" "$releases/0.140.0"
ln -s "$releases/0.144.5" "$home/.codex/packages/standalone/current"
printf '%s\n' "$releases/0.143.0/bin/codex" >"$home/.codex-hub/codex-target"
mkdir -p "$home/proc/321"
uid=$(id -u)
printf 'Uid:\t%s\t%s\t%s\t%s\n' "$uid" "$uid" "$uid" "$uid" >"$home/proc/321/status"
printf '321 (codex) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1001\n' >"$home/proc/321/stat"
ln -s "$releases/0.139.0/bin/codex" "$home/proc/321/exe"
HOME="$home" codexhub_cleanup_policy=verified-older-than codexhub_cleanup_verified_version=9.9.9 sh "$root/cleanup.sh" >"$root/first.out"
HOME="$home" codexhub_cleanup_policy=verified-older-than codexhub_cleanup_verified_version=9.9.9 sh "$root/cleanup.sh" >"$root/second.out"
cat "$root/first.out"
printf '%s\n' CODEXHUB_TEST_SECOND_RUN
cat "$root/second.out"
for item in \
  "OLD:$releases/0.142.5" \
  "SUFFIX_OLD:$releases/0.138.0-x86_64-unknown-linux-musl" \
  "CURRENT:$releases/0.144.5" \
  "TARGET:$releases/0.143.0" \
  "UNMARKED:$releases/0.141.0" \
  "SYMLINK:$releases/0.140.0" \
  "UNSAFE:$releases/unsafe_name" \
  "BAD_SUFFIX:$releases/0.137.0bad-x86_64-unknown-linux-musl" \
  "AMBIGUOUS:$releases/0.136.0" \
  "STRICT_UNMARKED:$releases/0.135.0" \
  "IN_USE:$releases/0.139.0"
do
  label=${item%%:*}
  path=${item#*:}
  if [ -e "$path" ] || [ -L "$path" ]; then state=present; else state=absent; fi
  printf 'CODEXHUB_TEST_%s=%s\n' "$label" "$state"
done
rm -rf "$root"
"###
        .replace("__CLEANUP_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "cleanup fixture failed: {}",
            output.stderr
        );
        let (first, second) = output
            .stdout
            .split_once("CODEXHUB_TEST_SECOND_RUN\n")
            .expect("two cleanup runs");
        let first_result = parse_remote_codex_release_cleanup_output(&ssh::SshCommandOutput {
            stdout: first.into(),
            ..output.clone()
        })
        .expect("first cleanup result");
        let second_result = parse_remote_codex_release_cleanup_output(&ssh::SshCommandOutput {
            stdout: second.into(),
            ..output.clone()
        })
        .expect("second cleanup result");
        assert_eq!(first_result.removed_count, 2);
        assert_eq!(first_result.adopted_count, 0);
        assert_eq!(first_result.retained_count, 9);
        assert_eq!(second_result.removed_count, 0);
        assert!(second.contains("CODEXHUB_TEST_OLD=absent"));
        assert!(second.contains("CODEXHUB_TEST_SUFFIX_OLD=absent"));
        for label in [
            "CURRENT",
            "TARGET",
            "UNMARKED",
            "SYMLINK",
            "UNSAFE",
            "BAD_SUFFIX",
            "AMBIGUOUS",
            "STRICT_UNMARKED",
            "IN_USE",
        ] {
            assert!(second.contains(&format!("CODEXHUB_TEST_{label}=present")));
        }
    }

    #[test]
    fn update_cleanup_adopts_every_strict_lower_release_and_preserves_protected_or_uncertain_entries(
    ) {
        let generated = remote_codex_release_cleanup_script_with_policy(
            &CodexReleaseCleanupPolicy::VerifiedOlderThan("codex-cli 0.145.0".into()),
        )
        .expect("verified update cleanup script")
        .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        let managed_retry = remote_codex_release_cleanup_script_with_policy(
            &CodexReleaseCleanupPolicy::ManagedOnly,
        )
        .expect("managed retry cleanup script")
        .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
hub="$home/.codex-hub"
mkdir -p "$releases" "$hub" "$home/proc"
cat >"$root/update-cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__UPDATE_CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
cat >"$root/managed-cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__MANAGED_CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
make_release() {
  entry=$1
  version=$2
  binary_relative_path=$3
  marker_mode=$4
  mkdir -p "$releases/$entry"
  binary_parent=${binary_relative_path%/*}
  if [ "$binary_parent" != "$binary_relative_path" ]; then mkdir -p "$releases/$entry/$binary_parent"; fi
  cat >"$releases/$entry/$binary_relative_path" <<CODEXHUB_TEST_BIN
#!/bin/sh
printf 'codex-cli $version\n'
CODEXHUB_TEST_BIN
  chmod 700 "$releases/$entry/$binary_relative_path"
  case "$marker_mode" in
    valid) printf 'CodexHub managed standalone release v1\nversion=%s\n' "$version" >"$releases/$entry/.codexhub-managed-release" ;;
    invalid) printf 'unmanaged marker\n' >"$releases/$entry/.codexhub-managed-release" ;;
    none) ;;
  esac
}
make_release 0.139.0 0.139.0 bin/codex valid
make_release 0.140.0 0.140.0 bin/codex none
make_release 0.141.0-x86_64-unknown-linux-musl 0.141.0 codex none
make_release 0.142.0 0.142.0 bin/codex none
make_release 0.143.0 0.143.0 bin/codex invalid
make_release 0.144.0 0.144.0 bin/codex none
ln -s bin/codex "$releases/0.144.0/codex"
make_release 0.145.0 0.145.0 bin/codex none
make_release 0.145.0-other 0.145.0 bin/codex none
make_release 0.146.0 0.146.0 bin/codex none
ln -s "$releases/0.145.0" "$home/.codex/packages/standalone/current"
printf '%s\n' "$releases/0.145.0/bin/codex" >"$hub/codex-target"
mkdir -p "$home/.local/bin" "$home/.codex/tmp/arg0/codex-arg0fixture"
ln -s "$releases/0.140.0/bin/codex" "$home/.local/bin/codex.codexhub.bak.fixture"
ln -s "$releases/0.141.0-x86_64-unknown-linux-musl/codex" \
  "$home/.codex/tmp/arg0/codex-arg0fixture/apply_patch"
mkdir -p "$home/proc/321"
uid=$(id -u)
printf 'Uid:\t%s\t%s\t%s\t%s\n' "$uid" "$uid" "$uid" "$uid" >"$home/proc/321/status"
printf '321 (codex) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1001\n' >"$home/proc/321/stat"
ln -s "$releases/0.142.0/bin/codex" "$home/proc/321/exe"
set +e
HOME="$home" sh "$root/update-cleanup.sh" >"$root/first.out"
first_status=$?
set -e
if [ "$first_status" -ne 0 ]; then
  cat "$root/first.out"
  exit "$first_status"
fi
backup=""
for item in "$hub"/deletion-backups/update-*; do
  [ -d "$item" ] || continue
  [ -z "$backup" ] || exit 92
  backup=$item
done
if [ -z "$backup" ]; then
  cat "$root/first.out"
  exit 93
fi
if [ "$(sed -n '1p' "$releases/0.142.0/.codexhub-managed-release" 2>/dev/null)" = "CodexHub managed standalone release v1" ]; then
  in_use_marker=valid
else
  in_use_marker=missing
fi
rm -rf "$home/proc/321"
set +e
HOME="$home" sh "$root/managed-cleanup.sh" >"$root/second.out"
second_status=$?
set -e
if [ "$second_status" -ne 0 ]; then
  cat "$root/first.out"
  printf '%s\n' CODEXHUB_TEST_SECOND_RUN
  cat "$root/second.out"
  exit "$second_status"
fi
cat "$root/first.out"
printf '%s\n' CODEXHUB_TEST_SECOND_RUN
cat "$root/second.out"
printf 'CODEXHUB_TEST_IN_USE_MARKER=%s\n' "$in_use_marker"
for item in \
  "MARKED_OLD:$releases/0.139.0" \
  "UNMARKED_OLD:$releases/0.140.0" \
  "VENDOR_OLD:$releases/0.141.0-x86_64-unknown-linux-musl" \
  "IN_USE_OLD:$releases/0.142.0" \
  "INVALID_MARKER:$releases/0.143.0" \
  "AMBIGUOUS:$releases/0.144.0" \
  "CURRENT:$releases/0.145.0" \
  "EQUAL:$releases/0.145.0-other" \
  "HIGHER:$releases/0.146.0"
do
  label=${item%%:*}
  path=${item#*:}
  if [ -e "$path" ] || [ -L "$path" ]; then state=present; else state=absent; fi
  printf 'CODEXHUB_TEST_%s=%s\n' "$label" "$state"
done
printf 'CODEXHUB_TEST_INVALID_MARKER_TEXT=%s\n' "$(sed -n '1p' "$releases/0.143.0/.codexhub-managed-release")"
if [ -e "$releases/0.145.0/.codexhub-managed-release" ]; then current_marker=present; else current_marker=absent; fi
if [ -e "$releases/0.145.0-other/.codexhub-managed-release" ]; then equal_marker=present; else equal_marker=absent; fi
if [ -e "$releases/0.146.0/.codexhub-managed-release" ]; then higher_marker=present; else higher_marker=absent; fi
printf 'CODEXHUB_TEST_CURRENT_MARKER=%s\n' "$current_marker"
printf 'CODEXHUB_TEST_EQUAL_MARKER=%s\n' "$equal_marker"
printf 'CODEXHUB_TEST_HIGHER_MARKER=%s\n' "$higher_marker"
for item in \
  "BACKUP_MARKED_OLD:$backup/releases/0.139.0" \
  "BACKUP_UNMARKED_OLD:$backup/releases/0.140.0" \
  "BACKUP_VENDOR_OLD:$backup/releases/0.141.0-x86_64-unknown-linux-musl" \
  "BACKUP_LOCAL_LINK:$backup/links/local-bin/codex.codexhub.bak.fixture" \
  "BACKUP_TMP_LINK:$backup/links/codex-tmp-arg0/codex-arg0fixture/apply_patch"
do
  label=${item%%:*}
  path=${item#*:}
  if [ -e "$path" ] || [ -L "$path" ]; then state=present; else state=absent; fi
  printf 'CODEXHUB_TEST_%s=%s\n' "$label" "$state"
done
if [ -e "$home/.local/bin/codex.codexhub.bak.fixture" ] || [ -L "$home/.local/bin/codex.codexhub.bak.fixture" ]; then
  live_local_link=present
else
  live_local_link=absent
fi
if [ -e "$home/.codex/tmp/arg0/codex-arg0fixture/apply_patch" ] || [ -L "$home/.codex/tmp/arg0/codex-arg0fixture/apply_patch" ]; then
  live_tmp_link=present
else
  live_tmp_link=absent
fi
printf 'CODEXHUB_TEST_LIVE_LOCAL_LINK=%s\n' "$live_local_link"
printf 'CODEXHUB_TEST_LIVE_TMP_LINK=%s\n' "$live_tmp_link"
rm -rf "$root"
"###
        .replace("__UPDATE_CLEANUP_SCRIPT__", &generated)
        .replace("__MANAGED_CLEANUP_SCRIPT__", &managed_retry);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "update cleanup fixture failed: stdout={} stderr={}",
            output.stdout,
            output.stderr
        );
        let (first, second) = output
            .stdout
            .split_once("CODEXHUB_TEST_SECOND_RUN\n")
            .expect("two update cleanup runs");
        let first_result = parse_remote_codex_release_cleanup_output(&ssh::SshCommandOutput {
            stdout: first.into(),
            ..output.clone()
        })
        .expect("first update cleanup result");
        let second_result = parse_remote_codex_release_cleanup_output(&ssh::SshCommandOutput {
            stdout: second.into(),
            ..output.clone()
        })
        .expect("second update cleanup result");
        assert_eq!(first_result.adopted_count, 3);
        assert_eq!(first_result.removed_count, 3);
        assert_eq!(first_result.backed_up_count, 3);
        assert!(first_result.backup_id.is_some());
        assert_eq!(first_result.retained_count, 6);
        assert_eq!(second_result.adopted_count, 0);
        assert_eq!(second_result.removed_count, 1);
        assert_eq!(second_result.backed_up_count, 0);
        assert!(second_result.backup_id.is_none());
        assert_eq!(second_result.retained_count, 5);
        assert!(second.contains("CODEXHUB_TEST_IN_USE_MARKER=valid"));
        for label in ["MARKED_OLD", "UNMARKED_OLD", "VENDOR_OLD", "IN_USE_OLD"] {
            assert!(second.contains(&format!("CODEXHUB_TEST_{label}=absent")));
        }
        for label in ["INVALID_MARKER", "AMBIGUOUS", "CURRENT", "EQUAL", "HIGHER"] {
            assert!(second.contains(&format!("CODEXHUB_TEST_{label}=present")));
        }
        assert!(second.contains("CODEXHUB_TEST_INVALID_MARKER_TEXT=unmanaged marker"));
        assert!(second.contains("CODEXHUB_TEST_CURRENT_MARKER=absent"));
        assert!(second.contains("CODEXHUB_TEST_EQUAL_MARKER=absent"));
        assert!(second.contains("CODEXHUB_TEST_HIGHER_MARKER=absent"));
        for label in [
            "BACKUP_MARKED_OLD",
            "BACKUP_UNMARKED_OLD",
            "BACKUP_VENDOR_OLD",
            "BACKUP_LOCAL_LINK",
            "BACKUP_TMP_LINK",
        ] {
            assert!(second.contains(&format!("CODEXHUB_TEST_{label}=present")));
        }
        assert!(second.contains("CODEXHUB_TEST_LIVE_LOCAL_LINK=absent"));
        assert!(second.contains("CODEXHUB_TEST_LIVE_TMP_LINK=absent"));
    }

    #[test]
    fn staged_release_backup_fails_if_the_original_path_is_recreated() {
        let mut generated = remote_codex_release_cleanup_script_with_policy(
            &CodexReleaseCleanupPolicy::VerifiedOlderThan("codex-cli 0.145.0".into()),
        )
        .expect("verified update cleanup script")
        .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        let final_recheck_anchor =
            "    # Close the zero-link and post-loop recreation window before completion.";
        assert_eq!(generated.matches(final_recheck_anchor).count(), 1);
        generated = generated.replacen(
            final_recheck_anchor,
            r###"    mkdir -p "$candidate/bin"
    cat >"$candidate/bin/codex" <<'CODEXHUB_RACED_RELEASE'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_RACED_RELEASE
    chmod 700 "$candidate/bin/codex"
    printf 'CodexHub managed standalone release v1\nversion=0.142.5\n' >"$candidate/.codexhub-managed-release"
    # Close the zero-link and post-loop recreation window before completion."###,
            1,
        );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
hub="$home/.codex-hub"
mkdir -p "$releases/0.142.5/bin" "$releases/0.145.0/bin" "$hub" "$home/proc"
for version in 0.142.5 0.145.0; do
  cat >"$releases/$version/bin/codex" <<CODEXHUB_TEST_BIN
#!/bin/sh
printf 'codex-cli $version\n'
CODEXHUB_TEST_BIN
  chmod 700 "$releases/$version/bin/codex"
  printf 'CodexHub managed standalone release v1\nversion=%s\n' "$version" >"$releases/$version/.codexhub-managed-release"
done
ln -s "$releases/0.145.0" "$home/.codex/packages/standalone/current"
printf '%s\n' "$releases/0.145.0/bin/codex" >"$hub/codex-target"
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
set +e
HOME="$home" sh "$root/cleanup.sh" >"$root/out"
status=$?
set -e
backup=""
for item in "$hub"/deletion-backups/update-*; do
  [ -d "$item" ] || continue
  backup=$item
done
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
if [ -d "$releases/0.142.5" ]; then active=present; else active=absent; fi
if [ -n "$backup" ] && [ -d "$backup/releases/0.142.5" ]; then staged=present; else staged=absent; fi
printf 'CODEXHUB_TEST_ACTIVE=%s\n' "$active"
printf 'CODEXHUB_TEST_STAGED=%s\n' "$staged"
rm -rf "$root"
exit 0
"###
        .replace("__CLEANUP_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "release recreation fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_release_cleanup_output(&ssh::SshCommandOutput {
            exit_code: Some(1),
            ..output.clone()
        })
        .expect("release recreation cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Failed);
        assert_eq!(parsed.reason, "update-backup-original-raced");
        assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=1"));
        assert!(output.stdout.contains("CODEXHUB_TEST_ACTIVE=present"));
        assert!(output.stdout.contains("CODEXHUB_TEST_STAGED=present"));
    }

    #[test]
    fn staged_capture_backup_preserves_a_recreated_capture_and_marker() {
        let mut generated = remote_codex_release_cleanup_script_with_policy(
            &CodexReleaseCleanupPolicy::VerifiedOlderThan("codex-cli 0.145.0".into()),
        )
        .expect("verified update cleanup script")
        .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        let marker_unlink_anchor =
            "    # Bind the final unlink to the marker that was hard-linked into the backup.";
        assert_eq!(generated.matches(marker_unlink_anchor).count(), 1);
        generated = generated.replacen(
            marker_unlink_anchor,
            r###"    rm -f "$capture_marker"
    cat >"$candidate" <<'CODEXHUB_RACED_CAPTURE'
#!/bin/sh
printf 'codex-cli 0.141.0\n'
CODEXHUB_RACED_CAPTURE
    chmod 700 "$candidate"
    printf 'CodexHub managed launcher capture v1\nname=%s\nversion=0.141.0\n' "$candidate_name" >"$capture_marker"
    # Bind the final unlink to the marker that was hard-linked into the backup."###,
            1,
        );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
releases="$home/.codex/packages/standalone/releases"
hub="$home/.codex-hub"
name=codex-original.20260101000000.401
mkdir -p "$releases/0.145.0/bin" "$hub" "$home/proc"
cat >"$releases/0.145.0/bin/codex" <<'CODEXHUB_CURRENT_BIN'
#!/bin/sh
printf 'codex-cli 0.145.0\n'
CODEXHUB_CURRENT_BIN
cat >"$hub/$name" <<'CODEXHUB_CAPTURE_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_CAPTURE_BIN
chmod 700 "$releases/0.145.0/bin/codex" "$hub/$name"
printf 'CodexHub managed standalone release v1\nversion=0.145.0\n' >"$releases/0.145.0/.codexhub-managed-release"
printf 'CodexHub managed launcher capture v1\nname=%s\nversion=0.142.5\n' "$name" >"$hub/$name.codexhub-managed-capture"
ln -s "$releases/0.145.0" "$home/.codex/packages/standalone/current"
printf '%s\n' "$releases/0.145.0/bin/codex" >"$hub/codex-target"
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
set +e
HOME="$home" sh "$root/cleanup.sh" >"$root/out"
status=$?
set -e
backup=""
for item in "$hub"/deletion-backups/update-*; do
  [ -d "$item" ] || continue
  backup=$item
done
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
if [ -f "$hub/$name" ]; then active=present; else active=absent; fi
if [ "$(sed -n '3p' "$hub/$name.codexhub-managed-capture" 2>/dev/null)" = 'version=0.141.0' ]; then marker=new; else marker=changed; fi
if [ -n "$backup" ] && [ -f "$backup/captures/$name" ] && [ -f "$backup/captures/$name.codexhub-managed-capture" ]; then staged=present; else staged=absent; fi
printf 'CODEXHUB_TEST_ACTIVE=%s\n' "$active"
printf 'CODEXHUB_TEST_MARKER=%s\n' "$marker"
printf 'CODEXHUB_TEST_STAGED=%s\n' "$staged"
rm -rf "$root"
exit 0
"###
        .replace("__CLEANUP_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "capture recreation fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_release_cleanup_output(&ssh::SshCommandOutput {
            exit_code: Some(1),
            ..output.clone()
        })
        .expect("capture recreation cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Failed);
        assert_eq!(parsed.reason, "capture-marker-identity-changed");
        assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=1"));
        assert!(output.stdout.contains("CODEXHUB_TEST_ACTIVE=present"));
        assert!(output.stdout.contains("CODEXHUB_TEST_MARKER=new"));
        assert!(output.stdout.contains("CODEXHUB_TEST_STAGED=present"));
    }

    #[test]
    fn capture_cleanup_handles_target_process_marker_and_unmanaged_boundaries() {
        let reconcile = remote_codex_runtime_reconcile_script();
        let cleanup = remote_codex_release_cleanup_script()
            .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
hub="$home/.codex-hub"
release="$home/.codex/packages/standalone/releases/0.144.5"
mkdir -p "$home/.local/bin" "$hub" "$home/proc" "$release/bin"
cat >"$release/bin/codex" <<'CODEXHUB_CURRENT_BIN'
#!/bin/sh
printf 'codex-cli 0.144.5\n'
CODEXHUB_CURRENT_BIN
cat >"$home/.local/bin/codex" <<'CODEXHUB_EXTERNAL_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_EXTERNAL_BIN
chmod 700 "$release/bin/codex" "$home/.local/bin/codex"
ln -s "$release" "$home/.codex/packages/standalone/current"
printf '%s\n' 'PATH="$HOME/.local/bin:$PATH"; export PATH' >"$home/.profile"
cat >"$root/reconcile.sh" <<'CODEXHUB_RECONCILE_SCRIPT'
__RECONCILE_SCRIPT__
CODEXHUB_RECONCILE_SCRIPT
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
HOME="$home" SHELL=/bin/sh PATH="$home/.local/bin:$PATH" sh "$root/reconcile.sh" >"$root/reconcile.out"
eligible=""
for item in "$hub"/codex-original.*; do
  [ -f "$item" ] || continue
  case "$item" in *.codexhub-managed-capture) continue ;; esac
  eligible=$item
done
[ -n "$eligible" ]
printf 'CODEXHUB_TEST_TARGET_AFTER_RECONCILE=%s\n' "$(sed -n '1p' "$hub/codex-target")" >>"$root/reconcile.out"
make_capture() {
  name=$1
  version=$2
  marker_version=$3
  cat >"$hub/$name" <<CODEXHUB_CAPTURE_BIN
#!/bin/sh
printf 'codex-cli $version\\n'
CODEXHUB_CAPTURE_BIN
  chmod 700 "$hub/$name"
  if [ "$marker_version" != none ]; then
    printf 'CodexHub managed launcher capture v1\nname=%s\nversion=%s\n' "$name" "$marker_version" >"$hub/$name.codexhub-managed-capture"
  fi
}
active=codex-original.20260101000000.101
running=codex-original.20260101000000.102
mismatch=codex-original.20260101000000.103
unmarked=codex-original.20260101000000.104
unsafe=codex-original.legacy
make_capture "$active" 0.140.0 0.140.0
make_capture "$running" 0.139.0 0.139.0
make_capture "$mismatch" 0.138.0 9.9.9
make_capture "$unmarked" 0.137.0 none
make_capture "$unsafe" 0.136.0 none
printf '%s\n' "$hub/$active" >"$hub/codex-target"
mkdir -p "$home/proc/321"
uid=$(id -u)
printf 'Uid:\t%s\t%s\t%s\t%s\n' "$uid" "$uid" "$uid" "$uid" >"$home/proc/321/status"
printf '321 (codex) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1001\n' >"$home/proc/321/stat"
ln -s "$hub/$running" "$home/proc/321/exe"
HOME="$home" sh "$root/cleanup.sh" >"$root/cleanup.out"
cat "$root/reconcile.out"
printf 'CODEXHUB_TEST_CLEANUP\n'
cat "$root/cleanup.out"
for item in \
  "ELIGIBLE:$eligible" \
  "ELIGIBLE_MARKER:$eligible.codexhub-managed-capture" \
  "ACTIVE:$hub/$active" \
  "RUNNING:$hub/$running" \
  "MISMATCH:$hub/$mismatch" \
  "UNMARKED:$hub/$unmarked" \
  "UNSAFE:$hub/$unsafe"
do
  label=${item%%:*}
  path=${item#*:}
  if [ -e "$path" ] || [ -L "$path" ]; then state=present; else state=absent; fi
  printf 'CODEXHUB_TEST_%s=%s\n' "$label" "$state"
done
rm -rf "$root"
"###
        .replace("__RECONCILE_SCRIPT__", reconcile)
        .replace("__CLEANUP_SCRIPT__", &cleanup);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "capture cleanup fixture failed: {}",
            output.stderr
        );
        let (_, cleanup_output) = output
            .stdout
            .split_once("CODEXHUB_TEST_CLEANUP\n")
            .expect("capture cleanup delimiter");
        let parsed = parse_remote_codex_release_cleanup_output(&ssh::SshCommandOutput {
            stdout: cleanup_output.into(),
            ..output.clone()
        })
        .expect("capture cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Completed);
        assert_eq!(parsed.scanned_count, 7);
        assert_eq!(parsed.removed_count, 1);
        assert_eq!(parsed.retained_count, 6);
        assert!(output
            .stdout
            .contains("CODEXHUB_TEST_TARGET_AFTER_RECONCILE="));
        assert!(output
            .stdout
            .contains("/.codex/packages/standalone/current/bin/codex"));
        assert!(cleanup_output.contains("CODEXHUB_TEST_ELIGIBLE=absent"));
        assert!(cleanup_output.contains("CODEXHUB_TEST_ELIGIBLE_MARKER=absent"));
        for label in ["ACTIVE", "RUNNING", "MISMATCH", "UNMARKED", "UNSAFE"] {
            assert!(cleanup_output.contains(&format!("CODEXHUB_TEST_{label}=present")));
        }
    }

    #[test]
    fn capture_cleanup_restores_quarantine_when_candidate_becomes_active() {
        let generated = remote_codex_release_cleanup_script()
            .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"")
            .replace(
                "  # The original launch path is hidden; now repeat every mutable safety check.",
                "  mkdir -p \"$proc_root/654\"\n  uid=$(id -u)\n  printf 'Uid:\\t%s\\t%s\\t%s\\t%s\\n' \"$uid\" \"$uid\" \"$uid\" \"$uid\" >\"$proc_root/654/status\"\n  printf '654 (codex) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 2001\\n' >\"$proc_root/654/stat\"\n  ln -s \"$quarantine\" \"$proc_root/654/exe\"\n  # The original launch path is hidden; now repeat every mutable safety check.",
            );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
hub="$home/.codex-hub"
release="$home/.codex/packages/standalone/releases/0.144.5"
name=codex-original.20260101000000.201
mkdir -p "$hub" "$home/proc" "$release/bin"
cat >"$release/bin/codex" <<'CODEXHUB_CURRENT_BIN'
#!/bin/sh
printf 'codex-cli 0.144.5\n'
CODEXHUB_CURRENT_BIN
cat >"$hub/$name" <<'CODEXHUB_CAPTURE_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_CAPTURE_BIN
chmod 700 "$release/bin/codex" "$hub/$name"
ln -s "$release" "$home/.codex/packages/standalone/current"
printf 'CodexHub managed standalone release v1\nversion=0.144.5\n' >"$release/.codexhub-managed-release"
printf 'CodexHub managed launcher capture v1\nname=%s\nversion=0.142.5\n' "$name" >"$hub/$name.codexhub-managed-capture"
printf '%s\n' "$home/.codex/packages/standalone/current/bin/codex" >"$hub/codex-target"
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
HOME="$home" sh "$root/cleanup.sh" >"$root/out"
cat "$root/out"
if [ -f "$hub/$name" ]; then capture=present; else capture=absent; fi
quarantine_count=0
for item in "$hub"/.codexhub-capture-quarantine.*; do
  [ -e "$item" ] || [ -L "$item" ] || continue
  quarantine_count=$((quarantine_count + 1))
done
printf 'CODEXHUB_TEST_CAPTURE=%s\n' "$capture"
printf 'CODEXHUB_TEST_QUARANTINE_COUNT=%s\n' "$quarantine_count"
rm -rf "$root"
"###
        .replace("__CLEANUP_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "capture race fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_release_cleanup_output(&output)
            .expect("deferred capture race result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Deferred);
        assert_eq!(parsed.reason, "capture-became-active");
        assert!(output.stdout.contains("CODEXHUB_TEST_CAPTURE=present"));
        assert!(output.stdout.contains("CODEXHUB_TEST_QUARANTINE_COUNT=0"));
    }

    #[test]
    fn capture_cleanup_recovers_only_fully_marked_orphan_quarantine() {
        let generated = remote_codex_release_cleanup_script()
            .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
hub="$home/.codex-hub"
name=codex-original.20260101000000.301
quarantine="$hub/.codexhub-capture-quarantine.$name.777"
mkdir -p "$hub" "$home/proc"
cat >"$quarantine" <<'CODEXHUB_CAPTURE_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_CAPTURE_BIN
chmod 700 "$quarantine"
printf 'CodexHub managed launcher capture v1\nname=%s\nversion=0.142.5\n' "$name" >"$hub/$name.codexhub-managed-capture"
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
HOME="$home" sh "$root/cleanup.sh" >"$root/out"
cat "$root/out"
if [ -e "$quarantine" ]; then quarantine_state=present; else quarantine_state=absent; fi
if [ -e "$hub/$name" ]; then capture_state=present; else capture_state=absent; fi
if [ -e "$hub/$name.codexhub-managed-capture" ]; then marker_state=present; else marker_state=absent; fi
printf 'CODEXHUB_TEST_QUARANTINE=%s\n' "$quarantine_state"
printf 'CODEXHUB_TEST_CAPTURE=%s\n' "$capture_state"
printf 'CODEXHUB_TEST_MARKER=%s\n' "$marker_state"
rm -rf "$root"
"###
        .replace("__CLEANUP_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "orphan capture fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_release_cleanup_output(&output)
            .expect("orphan capture cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Completed);
        assert_eq!(parsed.removed_count, 1);
        assert!(output.stdout.contains("CODEXHUB_TEST_QUARANTINE=absent"));
        assert!(output.stdout.contains("CODEXHUB_TEST_CAPTURE=absent"));
        assert!(output.stdout.contains("CODEXHUB_TEST_MARKER=absent"));
    }

    #[test]
    fn cleanup_defers_when_proc_is_unavailable_or_candidate_races() {
        let base = remote_codex_release_cleanup_script()
            .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"");
        for (name, script, expected_reason) in [
            ("proc", base.clone(), "proc-unavailable"),
            (
                "race",
                base.replace(
                    "  if ! verify_candidate \"$candidate\" ||",
                    "  rm -f \"$candidate/$marker_name\"\n  if ! verify_candidate \"$candidate\" ||",
                ),
                "candidate-raced",
            ),
            (
                "isolated-race",
                base.replace(
                    "  # Once isolated, ordinary launch paths can no longer start this release.",
                    "  mkdir -p \"$proc_root/654\"\n  uid=$(id -u)\n  printf 'Uid:\\t%s\\t%s\\t%s\\t%s\\n' \"$uid\" \"$uid\" \"$uid\" \"$uid\" >\"$proc_root/654/status\"\n  printf '654 (codex) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 2001\\n' >\"$proc_root/654/stat\"\n  ln -s \"$quarantine/bin/codex\" \"$proc_root/654/exe\"\n  # Once isolated, ordinary launch paths can no longer start this release.",
                ),
                "candidate-became-active",
            ),
        ] {
            let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
release="$home/.codex/packages/standalone/releases/0.142.5"
mkdir -p "$release/bin" "$home/.codex-hub"
cat >"$release/bin/codex" <<'CODEXHUB_TEST_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_TEST_BIN
chmod 700 "$release/bin/codex"
printf 'CodexHub managed standalone release v1\nversion=0.142.5\n' >"$release/.codexhub-managed-release"
            if [ "__NAME__" != proc ]; then mkdir -p "$home/proc"; fi
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
HOME="$home" sh "$root/cleanup.sh"
if [ -d "$release" ]; then printf 'CODEXHUB_TEST_RELEASE=present\n'; else printf 'CODEXHUB_TEST_RELEASE=absent\n'; fi
rm -rf "$root"
"###
            .replace("__NAME__", name)
            .replace("__CLEANUP_SCRIPT__", &script);
            let Some(output) = run_sh(&harness) else {
                return;
            };
            assert!(output.success(), "{name} fixture failed: {}", output.stderr);
            let parsed = parse_remote_codex_release_cleanup_output(&output)
                .expect("deferred cleanup result");
            assert_eq!(parsed.status, CodexReleaseCleanupStatus::Deferred);
            assert_eq!(parsed.reason, expected_reason);
            assert!(output.stdout.contains("CODEXHUB_TEST_RELEASE=present"));
            assert!(!output.stdout.contains(".codexhub-quarantine."));
        }
    }

    #[test]
    fn cleanup_defers_on_pid_reuse_during_uid_starttime_and_exe_verification() {
        let generated = remote_codex_release_cleanup_script()
            .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"")
            .replace(
                "  read_cleanup_process_uid_starttime \"$proc_dir\"\n  proc_identity_after_status=$?",
                "  printf '654 (codex) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 2002\\n' >\"$proc_dir/stat\"\n  read_cleanup_process_uid_starttime \"$proc_dir\"\n  proc_identity_after_status=$?",
            );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
release="$home/.codex/packages/standalone/releases/0.142.5"
mkdir -p "$release/bin" "$home/.codex-hub" "$home/proc/654"
cat >"$release/bin/codex" <<'CODEXHUB_TEST_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_TEST_BIN
chmod 700 "$release/bin/codex"
printf 'CodexHub managed standalone release v1\nversion=0.142.5\n' >"$release/.codexhub-managed-release"
uid=$(id -u)
printf 'Uid:\t%s\t%s\t%s\t%s\n' "$uid" "$uid" "$uid" "$uid" >"$home/proc/654/status"
printf '654 (codex) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1001\n' >"$home/proc/654/stat"
ln -s "$release/bin/codex" "$home/proc/654/exe"
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
HOME="$home" sh "$root/cleanup.sh" >"$root/out"
cat "$root/out"
if [ -d "$release" ]; then printf 'CODEXHUB_TEST_RELEASE=present\n'; else printf 'CODEXHUB_TEST_RELEASE=absent\n'; fi
rm -rf "$root"
"###
        .replace("__CLEANUP_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "PID reuse fixture failed: {}",
            output.stderr
        );
        let parsed =
            parse_remote_codex_release_cleanup_output(&output).expect("PID reuse cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Deferred);
        assert_eq!(parsed.reason, "proc-identity-unknown");
        assert!(output.stdout.contains("CODEXHUB_TEST_RELEASE=present"));
    }

    #[test]
    fn cleanup_defers_when_non_user_pid_is_reused_by_current_uid() {
        let generated = remote_codex_release_cleanup_script()
            .replace("proc_root=/proc", "proc_root=\"$HOME/proc\"")
            .replace(
                "    read_cleanup_process_uid_starttime \"$proc_dir\"\n    non_user_identity_status=$?",
                "    current_uid=$(id -u)\n    printf 'Uid:\\t%s\\t%s\\t%s\\t%s\\n' \"$current_uid\" \"$current_uid\" \"$current_uid\" \"$current_uid\" >\"$proc_dir/status\"\n    printf '654 (codex) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 2002\\n' >\"$proc_dir/stat\"\n    read_cleanup_process_uid_starttime \"$proc_dir\"\n    non_user_identity_status=$?",
            );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
release="$home/.codex/packages/standalone/releases/0.142.5"
mkdir -p "$release/bin" "$home/.codex-hub" "$home/proc/654"
cat >"$release/bin/codex" <<'CODEXHUB_TEST_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_TEST_BIN
chmod 700 "$release/bin/codex"
printf 'CodexHub managed standalone release v1\nversion=0.142.5\n' >"$release/.codexhub-managed-release"
printf 'Uid:\t999999999\t999999999\t999999999\t999999999\n' >"$home/proc/654/status"
printf '654 (other) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1001\n' >"$home/proc/654/stat"
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
HOME="$home" sh "$root/cleanup.sh" >"$root/out"
cat "$root/out"
if [ -d "$release" ]; then printf 'CODEXHUB_TEST_RELEASE=present\n'; else printf 'CODEXHUB_TEST_RELEASE=absent\n'; fi
rm -rf "$root"
"###
        .replace("__CLEANUP_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "cross-UID PID reuse fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_release_cleanup_output(&output)
            .expect("cross-UID PID reuse cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Deferred);
        assert_eq!(parsed.reason, "proc-identity-unknown");
        assert!(output.stdout.contains("CODEXHUB_TEST_RELEASE=present"));
    }

    #[test]
    fn cleanup_fails_safely_when_gnu_mv_no_replace_is_unavailable() {
        let generated = remote_codex_release_cleanup_script()
            .replace(
                "mv -T -n \"$mv_probe_source\" \"$mv_probe_destination\" >/dev/null 2>&1",
                "false",
            )
            .replace(
                "mv -T -n \"$mv_probe_move_source\" \"$mv_probe_move_destination\" >/dev/null 2>&1",
                "false",
            );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
release="$home/.codex/packages/standalone/releases/0.142.5"
mkdir -p "$release/bin" "$home/.codex-hub"
cat >"$release/bin/codex" <<'CODEXHUB_TEST_BIN'
#!/bin/sh
printf 'codex-cli 0.142.5\n'
CODEXHUB_TEST_BIN
chmod 700 "$release/bin/codex"
printf 'CodexHub managed standalone release v1\nversion=0.142.5\n' >"$release/.codexhub-managed-release"
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
set +e
HOME="$home" sh "$root/cleanup.sh" >"$root/out"
status=$?
set -e
cat "$root/out"
printf 'CODEXHUB_TEST_STATUS=%s\n' "$status"
if [ -d "$release" ]; then printf 'CODEXHUB_TEST_RELEASE=present\n'; else printf 'CODEXHUB_TEST_RELEASE=absent\n'; fi
rm -rf "$root"
exit 0
"###
        .replace("__CLEANUP_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "unsupported mv fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_release_cleanup_output(&ssh::SshCommandOutput {
            exit_code: Some(1),
            ..output.clone()
        })
        .expect("unsupported mv cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::Failed);
        assert_eq!(parsed.reason, "mv-no-replace-unavailable");
        assert!(output.stdout.contains("CODEXHUB_TEST_STATUS=1"));
        assert!(output.stdout.contains("CODEXHUB_TEST_RELEASE=present"));
    }

    #[test]
    fn cleanup_accepts_coreutils_9_4_no_clobber_collision_status() {
        let collision_probe = "mv -T -n \"$mv_probe_source\" \"$mv_probe_destination\" >/dev/null 2>&1\n  mv_probe_status=$?";
        let mut generated = remote_codex_release_cleanup_script().to_string();
        assert_eq!(generated.matches(collision_probe).count(), 1);
        generated = generated.replacen(
            collision_probe,
            "mv -T -n \"$mv_probe_source\" \"$mv_probe_destination\" >/dev/null 2>&1\n  mv_probe_status=$?\n  mv_probe_status=1",
            1,
        );
        let harness = r###"set -eu
root=$(mktemp -d)
home="$root/home"
mkdir -p "$home"
cat >"$root/cleanup.sh" <<'CODEXHUB_CLEANUP_SCRIPT'
__CLEANUP_SCRIPT__
CODEXHUB_CLEANUP_SCRIPT
HOME="$home" sh "$root/cleanup.sh" >"$root/out"
cat "$root/out"
if [ -e "$home/.codexhub-runtime-cleanup.lock" ]; then lock_after=present; else lock_after=absent; fi
printf 'CODEXHUB_TEST_LOCK=%s\n' "$lock_after"
rm -rf "$root"
"###
        .replace("__CLEANUP_SCRIPT__", &generated);
        let Some(output) = run_sh(&harness) else {
            return;
        };
        assert!(
            output.success(),
            "coreutils 9.4 cleanup fixture failed: {}",
            output.stderr
        );
        let parsed = parse_remote_codex_release_cleanup_output(&output)
            .expect("coreutils 9.4 cleanup result");
        assert_eq!(parsed.status, CodexReleaseCleanupStatus::NotApplicable);
        assert_eq!(parsed.reason, "no-release-directories");
        assert!(output.stdout.contains("CODEXHUB_TEST_LOCK=absent"));
    }

    #[test]
    fn runtime_and_cleanup_scripts_are_posix_shell_syntax() {
        for script in [
            remote_codex_runtime_reconcile_script().to_string(),
            remote_codex_release_cleanup_script().to_string(),
            remote_codex_runtime_reconcile_script_with_minimum(Some("codex-cli 0.142.5"))
                .expect("minimum version script"),
        ] {
            let check = format!("sh -n <<'CODEXHUB_SCRIPT'\n{script}\nCODEXHUB_SCRIPT\n");
            let Some(output) = run_sh(&check) else {
                return;
            };
            assert!(output.success(), "sh -n failed: {}", output.stderr);
        }
    }
}
