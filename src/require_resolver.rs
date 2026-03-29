/*
Copyright (C) 2026 Yanlvl99 | Nicy Luau Runtime Development

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use mlua_sys::luau::compat;
use mlua_sys::luau::lauxlib;
use mlua_sys::luau::lua;
use mlua_sys::luau::luacodegen;
use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::fs;
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

type LuauState = lua::lua_State;
const NICY_CODEGEN_CREATED_KEY: &[u8] = b"nicy_codegen_created\0";

#[derive(Clone, Copy, Eq, PartialEq)]
struct FileFingerprint {
    modified_ns: u128,
    size: u64,
}

impl FileFingerprint {
    fn from_path(path: &Path) -> Result<Self, String> {
        let meta = fs::metadata(path)
            .map_err(|e| format!("failed to stat '{}': {}", path.display(), e))?;
        let size = meta.len();
        let modified_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Ok(Self { modified_ns, size })
    }
}

struct ModuleCacheEntry {
    fp: FileFingerprint,
    value_ref: c_int,
}

struct LuaurcCacheEntry {
    fp: FileFingerprint,
    aliases: HashMap<String, String>,
}

struct RuntimeData {
    entry_file: PathBuf,
    entry_dir: PathBuf,
    module_stack: Vec<PathBuf>,
    module_cache: HashMap<PathBuf, ModuleCacheEntry>,
    module_jit: HashMap<PathBuf, bool>,
    loading: HashSet<PathBuf>,
    luaurc_cache: HashMap<PathBuf, LuaurcCacheEntry>,
}

impl RuntimeData {
    fn new(entry_file: PathBuf, entry_dir: PathBuf) -> Self {
        Self {
            entry_file,
            entry_dir,
            module_stack: Vec::new(),
            module_cache: HashMap::new(),
            module_jit: HashMap::new(),
            loading: HashSet::new(),
            luaurc_cache: HashMap::new(),
        }
    }
}

static RUNTIMES: OnceLock<Mutex<HashMap<usize, RuntimeData>>> = OnceLock::new();

fn runtimes() -> &'static Mutex<HashMap<usize, RuntimeData>> {
    RUNTIMES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with_runtime<R>(l: *mut LuauState, f: impl FnOnce(&mut RuntimeData) -> R) -> Result<R, String> {
    let mut all = runtimes().lock().unwrap_or_else(|e| e.into_inner());
    let rt = all
        .get_mut(&(l as usize))
        .ok_or_else(|| "runtime context is not initialized".to_string())?;
    Ok(f(rt))
}

fn canonicalize_existing(path: PathBuf) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    fs::canonicalize(&path)
        .map_err(|e| format!("failed to canonicalize '{}': {}", path.display(), e))
}

fn resolve_loadlib_base(rt: &RuntimeData, current_module: &Path, spec: &str) -> Result<PathBuf, String> {
    if let Some(rest) = spec.strip_prefix("@self") {
        if rest.is_empty() {
            if is_init_module(current_module) {
                return Ok(
                    current_module
                        .parent()
                        .unwrap_or(rt.entry_dir.as_path())
                        .to_path_buf(),
                );
            }
            return Ok(current_module.to_path_buf());
        }
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        let root = if is_init_module(current_module) {
            current_module.parent().unwrap_or(rt.entry_dir.as_path()).to_path_buf()
        } else {
            current_module.parent().unwrap_or(rt.entry_dir.as_path()).to_path_buf()
        };
        return Ok(root.join(rest));
    }
    if spec.starts_with("./") || spec.starts_with("../") {
        let parent = current_module.parent().unwrap_or(rt.entry_dir.as_path());
        return Ok(parent.join(spec));
    }
    if spec.starts_with('@') {
        return Err("loadlib supports only @self aliases".to_string());
    }
    let p = Path::new(spec);
    if p.is_absolute() {
        return Ok(p.to_path_buf());
    }
    Ok(rt.entry_dir.join(p))
}

fn strip_quotes(v: &str) -> &str {
    let s = v.trim();
    let s = s.strip_prefix('[').unwrap_or(s).trim();
    let s = s.strip_suffix(']').unwrap_or(s).trim();
    let s = s.strip_prefix('"').unwrap_or(s);
    let s = s.strip_suffix('"').unwrap_or(s);
    let s = s.strip_prefix('\'').unwrap_or(s);
    s.strip_suffix('\'').unwrap_or(s)
}

fn parse_aliases_from_luaurc(content: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(aliases_idx) = content.find("aliases") else {
        return out;
    };
    let Some(start_rel) = content[aliases_idx..].find('{') else {
        return out;
    };
    let start = aliases_idx + start_rel;
    let mut depth = 0_i32;
    let mut end = None;
    for (i, ch) in content[start..].char_indices() {
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                end = Some(start + i);
                break;
            }
        }
    }
    let Some(end) = end else {
        return out;
    };
    let block = &content[start + 1..end];
    for raw_line in block.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with("--") || line.starts_with("//") {
            continue;
        }
        line = line.trim_end_matches(',');
        let (raw_key, raw_value) = if let Some(i) = line.find(':') {
            (&line[..i], &line[i + 1..])
        } else if let Some(i) = line.find('=') {
            (&line[..i], &line[i + 1..])
        } else {
            continue;
        };
        let key = strip_quotes(raw_key).trim();
        let value = strip_quotes(raw_value).trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        let normalized = if key.starts_with('@') {
            key.to_string()
        } else {
            format!("@{}", key)
        };
        out.insert(normalized, value.to_string());
    }
    out
}

fn aliases_for_dir(rt: &mut RuntimeData, dir: &Path) -> Result<HashMap<String, PathBuf>, String> {
    let mut chain = Vec::new();
    let mut cur = Some(dir);
    while let Some(d) = cur {
        chain.push(d.to_path_buf());
        cur = d.parent();
    }
    chain.reverse();

    let mut out = HashMap::<String, PathBuf>::new();
    for d in chain {
        let rc = d.join(".luaurc");
        if !rc.exists() {
            continue;
        }

        let fp = FileFingerprint::from_path(&rc)?;
        let aliases = if let Some(cached) = rt.luaurc_cache.get(&rc) {
            if cached.fp == fp {
                cached.aliases.clone()
            } else {
                let content = fs::read_to_string(&rc)
                    .map_err(|e| format!("failed to read '{}': {}", rc.display(), e))?;
                let parsed = parse_aliases_from_luaurc(&content);
                rt.luaurc_cache.insert(
                    rc.clone(),
                    LuaurcCacheEntry {
                        fp,
                        aliases: parsed.clone(),
                    },
                );
                parsed
            }
        } else {
            let content = fs::read_to_string(&rc)
                .map_err(|e| format!("failed to read '{}': {}", rc.display(), e))?;
            let parsed = parse_aliases_from_luaurc(&content);
            rt.luaurc_cache.insert(
                rc.clone(),
                LuaurcCacheEntry {
                    fp,
                    aliases: parsed.clone(),
                },
            );
            parsed
        };

        for (k, v) in aliases {
            let base = Path::new(&v);
            let abs = if base.is_absolute() {
                base.to_path_buf()
            } else {
                d.join(base)
            };
            out.insert(k, abs);
        }
    }
    Ok(out)
}

fn is_init_module(path: &Path) -> bool {
    matches!(path.file_name().and_then(|s| s.to_str()), Some("init.lua") | Some("init.luau"))
}

fn resolve_spec_base(rt: &mut RuntimeData, current_module: &Path, spec: &str) -> Result<PathBuf, String> {
    if spec.starts_with("./") || spec.starts_with("../") {
        let parent = current_module.parent().unwrap_or(rt.entry_dir.as_path());
        return Ok(parent.join(spec));
    }
    if spec == "." {
        if is_init_module(current_module) {
            return Ok(
                current_module
                    .parent()
                    .unwrap_or(rt.entry_dir.as_path())
                    .to_path_buf(),
            );
        }
        return Ok(current_module.to_path_buf());
    }
    if let Some(rest) = spec.strip_prefix("@self") {
        if rest.is_empty() {
            if is_init_module(current_module) {
                return Ok(
                    current_module
                        .parent()
                        .unwrap_or(rt.entry_dir.as_path())
                        .to_path_buf(),
                );
            }
            return Ok(current_module.to_path_buf());
        }
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        let self_root = if is_init_module(current_module) {
            current_module.parent().unwrap_or(rt.entry_dir.as_path()).to_path_buf()
        } else {
            current_module.parent().unwrap_or(rt.entry_dir.as_path()).to_path_buf()
        };
        return Ok(self_root.join(rest));
    }
    if let Some(alias_spec) = spec.strip_prefix('@') {
        let mut parts = alias_spec.splitn(2, '/');
        let alias_name = parts.next().unwrap_or_default();
        let remain = parts.next().unwrap_or_default();
        let key = format!("@{}", alias_name);
        let caller_dir = current_module
            .parent()
            .unwrap_or(rt.entry_dir.as_path())
            .to_path_buf();
        let aliases = aliases_for_dir(rt, &caller_dir)?;
        let base = aliases
            .get(&key)
            .cloned()
            .ok_or_else(|| format!("unknown alias '{}'", key))?;
        if remain.is_empty() {
            return Ok(base);
        }
        return Ok(base.join(remain));
    }
    Ok(rt.entry_dir.join(spec))
}

fn candidate_paths(base: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if base.extension().is_some() {
        out.push(base.to_path_buf());
        return out;
    }
    out.push(base.with_extension("luau"));
    out.push(base.with_extension("lua"));
    out.push(base.join("init.luau"));
    out.push(base.join("init.lua"));
    out
}

fn resolve_module_path(rt: &mut RuntimeData, current_module: &Path, spec: &str) -> Result<PathBuf, String> {
    let base = resolve_spec_base(rt, current_module, spec)?;
    let mut existing = Vec::new();
    for c in candidate_paths(&base) {
        if c.exists() {
            existing.push(canonicalize_existing(c)?);
        }
    }
    existing.sort();
    existing.dedup();
    if existing.is_empty() {
        return Err(format!(
            "module '{}' not found from '{}'",
            spec,
            current_module.display()
        ));
    }
    if existing.len() > 1 {
        let paths = existing
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("module '{}' is ambiguous: {}", spec, paths));
    }
    Ok(existing.remove(0))
}

fn lua_string_at(l: *mut LuauState, idx: c_int) -> Result<String, String> {
    let ptr = unsafe { lauxlib::luaL_checkstring(l, idx) };
    if ptr.is_null() {
        return Err("expected a string".to_string());
    }
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| "invalid utf-8 string".to_string())?;
    Ok(s.to_string())
}

fn push_error(l: *mut LuauState, msg: &str) -> c_int {
    let filtered = msg.replace('\0', "?");
    unsafe {
        lua::lua_pushnil(l);
        compat::lua_pushlstring(l, filtered.as_ptr() as *const c_char, filtered.len());
        2
    }
}

fn strip_native_directive(source: &str) -> (bool, String) {
    let mut lines = source.lines();
    let first = lines.next().unwrap_or("");
    let enabled = first.trim().starts_with("--!native");
    if !enabled {
        return (false, source.to_string());
    }
    let rest = lines.collect::<Vec<_>>().join("\n");
    (true, rest)
}

fn codegen_supported() -> bool {
    unsafe { luacodegen::luau_codegen_supported() != 0 }
}

fn codegen_created(l: *mut LuauState) -> bool {
    unsafe {
        lua::lua_getfield(
            l,
            lua::LUA_REGISTRYINDEX,
            NICY_CODEGEN_CREATED_KEY.as_ptr() as *const c_char,
        );
        let enabled = lua::lua_type(l, -1) != lua::LUA_TNIL && lua::lua_toboolean(l, -1) != 0;
        lua::lua_settop(l, -2);
        enabled
    }
}

pub fn ensure_codegen_context(l: *mut LuauState) -> bool {
    if !codegen_supported() {
        return false;
    }
    if !codegen_created(l) {
        unsafe { luacodegen::luau_codegen_create(l) };
        unsafe {
            lua::lua_pushboolean(l, 1);
            lua::lua_setfield(
                l,
                lua::LUA_REGISTRYINDEX,
                NICY_CODEGEN_CREATED_KEY.as_ptr() as *const c_char,
            );
        }
    }
    true
}

pub fn init_runtime(l: *mut LuauState, entry_file: &Path) -> Result<(), String> {
    let entry_file = canonicalize_existing(entry_file.to_path_buf())?;
    let entry_dir = entry_file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let mut all = runtimes().lock().unwrap_or_else(|e| e.into_inner());
    all.insert(l as usize, RuntimeData::new(entry_file, entry_dir));
    Ok(())
}

pub fn set_entry_jit(l: *mut LuauState, enabled: bool) -> Result<(), String> {
    with_runtime(l, |rt| {
        rt.module_jit.insert(rt.entry_file.clone(), enabled);
    })?;
    Ok(())
}

pub fn has_jit(l: *mut LuauState, spec: Option<&str>) -> bool {
    let lookup = with_runtime(l, |rt| {
        let current = rt
            .module_stack
            .last()
            .cloned()
            .unwrap_or_else(|| rt.entry_file.clone());

        if let Some(spec) = spec {
            resolve_module_path(rt, &current, spec)
        } else {
            Ok(current)
        }
    });

    let Ok(Ok(path)) = lookup else {
        return false;
    };

    with_runtime(l, |rt| rt.module_jit.get(&path).copied().unwrap_or(false)).unwrap_or(false)
}

pub fn resolve_loadlib_path(l: *mut LuauState, spec: &str) -> Result<PathBuf, String> {
    with_runtime(l, |rt| {
        let current = rt
            .module_stack
            .last()
            .cloned()
            .unwrap_or_else(|| rt.entry_file.clone());
        resolve_loadlib_base(rt, &current, spec)
            .and_then(canonicalize_existing)
    })?
}

pub fn shutdown_runtime(l: *mut LuauState) {
    let mut all = runtimes().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(rt) = all.remove(&(l as usize)) {
        for entry in rt.module_cache.values() {
            unsafe {
                lauxlib::luaL_unref(l, lua::LUA_REGISTRYINDEX, entry.value_ref);
            }
        }
    }
}

pub fn push_entry_module(l: *mut LuauState) -> Result<(), String> {
    with_runtime(l, |rt| {
        rt.module_stack.push(rt.entry_file.clone());
    })?;
    Ok(())
}

pub fn pop_entry_module(l: *mut LuauState) {
    let _ = with_runtime(l, |rt| {
        rt.module_stack.pop();
    });
}

unsafe extern "C-unwind" fn nicy_require(l: *mut LuauState) -> c_int {
    let spec = match lua_string_at(l, 1) {
        Ok(s) => s,
        Err(e) => return push_error(l, &e),
    };

    let current_module = match with_runtime(l, |rt| rt.module_stack.last().cloned()) {
        Ok(Some(p)) => p,
        Ok(None) => return push_error(l, "require has no current module context"),
        Err(e) => return push_error(l, &e),
    };

    let resolved = match with_runtime(l, |rt| resolve_module_path(rt, &current_module, &spec)) {
        Ok(Ok(path)) => path,
        Ok(Err(e)) => return push_error(l, &e),
        Err(e) => return push_error(l, &e),
    };

    let current_fp = match FileFingerprint::from_path(&resolved) {
        Ok(fp) => fp,
        Err(e) => return push_error(l, &e),
    };

    {
        let cached_ref = match with_runtime(l, |rt| {
            rt.module_cache.get(&resolved).and_then(|entry| {
                if entry.fp == current_fp {
                    Some(entry.value_ref)
                } else {
                    None
                }
            })
        }) {
            Ok(v) => v,
            Err(e) => return push_error(l, &e),
        };

        if let Some(r) = cached_ref {
            unsafe { compat::lua_rawgeti(l, lua::LUA_REGISTRYINDEX, r as lua::lua_Integer) };
            return 1;
        }
    }

    let had_old_ref = match with_runtime(l, |rt| rt.module_cache.get(&resolved).map(|e| e.value_ref)) {
        Ok(v) => v,
        Err(e) => return push_error(l, &e),
    };
    if let Some(r) = had_old_ref {
        unsafe { lauxlib::luaL_unref(l, lua::LUA_REGISTRYINDEX, r) };
        let _ = with_runtime(l, |rt| {
            rt.module_cache.remove(&resolved);
            rt.module_jit.remove(&resolved);
        });
    }

    let cycle = match with_runtime(l, |rt| {
        if rt.loading.contains(&resolved) {
            true
        } else {
            rt.loading.insert(resolved.clone());
            false
        }
    }) {
        Ok(v) => v,
        Err(e) => return push_error(l, &e),
    };
    if cycle {
        return push_error(
            l,
            &format!("cyclic require detected for '{}'", resolved.display()),
        );
    }

    let source = match fs::read_to_string(&resolved) {
        Ok(c) => c,
        Err(e) => {
            let _ = with_runtime(l, |rt| {
                rt.loading.remove(&resolved);
            });
            return push_error(l, &format!("failed to read '{}': {}", resolved.display(), e));
        }
    };
    let (module_native_requested, code) = strip_native_directive(&source);
    let module_jit_enabled = module_native_requested && ensure_codegen_context(l);

    let mut chunkname = resolved.to_string_lossy().replace('\0', "?").into_bytes();
    chunkname.push(0);

    let load_status = unsafe {
        compat::luaL_loadbuffer(
            l,
            code.as_ptr() as *const c_char,
            code.as_bytes().len(),
            chunkname.as_ptr() as *const c_char,
        )
    };
    if load_status != 0 {
        let err = unsafe { lua::lua_tostring(l, -1) };
        let msg = if err.is_null() {
            "unknown load error".to_string()
        } else {
            unsafe { CStr::from_ptr(err) }.to_string_lossy().to_string()
        };
        unsafe { lua::lua_settop(l, -2) };
        let _ = with_runtime(l, |rt| {
            rt.loading.remove(&resolved);
        });
        return push_error(
            l,
            &format!("load error in '{}': {}", resolved.display(), msg),
        );
    }

    if module_jit_enabled {
        unsafe { luacodegen::luau_codegen_compile(l, -1) };
    }

    let _ = with_runtime(l, |rt| {
        rt.module_stack.push(resolved.clone());
    });

    let call_status = unsafe { lua::lua_pcall(l, 0, 1, 0) };

    let _ = with_runtime(l, |rt| {
        rt.module_stack.pop();
        rt.loading.remove(&resolved);
    });

    if call_status != 0 {
        let err = unsafe { lua::lua_tostring(l, -1) };
        let msg = if err.is_null() {
            "unknown runtime error".to_string()
        } else {
            unsafe { CStr::from_ptr(err) }.to_string_lossy().to_string()
        };
        unsafe { lua::lua_settop(l, -2) };
        return push_error(
            l,
            &format!("runtime error in '{}': {}", resolved.display(), msg),
        );
    }

    if unsafe { lua::lua_type(l, -1) } == lua::LUA_TNIL {
        unsafe { lua::lua_settop(l, -2) };
        unsafe { lua::lua_pushboolean(l, 1) };
    }

    let value_ref = unsafe { lauxlib::luaL_ref(l, lua::LUA_REGISTRYINDEX) };

    let cache_insert = with_runtime(l, |rt| {
        rt.module_cache
            .insert(resolved.clone(), ModuleCacheEntry { fp: current_fp, value_ref });
        rt.module_jit.insert(resolved.clone(), module_jit_enabled);
    });
    if let Err(e) = cache_insert {
        unsafe { lauxlib::luaL_unref(l, lua::LUA_REGISTRYINDEX, value_ref) };
        return push_error(l, &e);
    }

    unsafe { compat::lua_rawgeti(l, lua::LUA_REGISTRYINDEX, value_ref as lua::lua_Integer) };
    1
}

pub fn install_require(l: *mut LuauState) {
    unsafe { lua::lua_pushcfunction(l, nicy_require) };
    unsafe { lua::lua_setglobal(l, b"require\0".as_ptr() as *const c_char) };
}
