/*
Copyright (C) 2026 Yanlvl99 | Nicy Luau Runtime Development

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use libloading::{Library, Symbol};
use mlua_sys::luau::{compat, lauxlib, lua, lualib};
use std::ffi::CStr;
use std::ffi::CString;
use std::fs;
use std::os::raw::{c_char, c_int};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

mod ffi_exports;
mod require_resolver;
mod task_scheduler;

const RUNTIME_VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
const RUNTIME_VERSION_LABEL: &[u8] =
    concat!("Nicy Runtime ", env!("CARGO_PKG_VERSION"), "\0").as_bytes();

#[allow(non_snake_case)]
mod api {
    use super::{compat, lauxlib, lua, lualib};
    use std::os::raw::{c_char, c_int};

    pub type LuauState = lua::lua_State;
    pub const LUA_REGISTRYINDEX: c_int = lua::LUA_REGISTRYINDEX;
    pub const LUA_TNIL: c_int = lua::LUA_TNIL;
    pub const LUA_TTABLE: c_int = lua::LUA_TTABLE;

    pub unsafe fn lua_pushnil(l: *mut LuauState) {
        unsafe { lua::lua_pushnil(l) };
    }

    pub unsafe fn lua_pushlstring(l: *mut LuauState, s: *const c_char, len: usize) {
        unsafe { compat::lua_pushlstring(l, s, len) };
    }

    pub unsafe fn lua_tostring(l: *mut LuauState, idx: c_int) -> *const c_char {
        unsafe { lua::lua_tostring(l, idx) }
    }

    pub unsafe fn lua_gettop(l: *mut LuauState) -> c_int {
        unsafe { lua::lua_gettop(l) }
    }

    pub unsafe fn lua_getfield(l: *mut LuauState, idx: c_int, k: *const c_char) {
        unsafe { lua::lua_getfield(l, idx, k) };
    }

    pub unsafe fn lua_type(l: *mut LuauState, idx: c_int) -> c_int {
        unsafe { lua::lua_type(l, idx) }
    }

    pub unsafe fn lua_settop(l: *mut LuauState, idx: c_int) {
        unsafe { lua::lua_settop(l, idx) };
    }

    pub unsafe fn lua_createtable(l: *mut LuauState, narr: c_int, nrec: c_int) {
        unsafe { lua::lua_createtable(l, narr, nrec) };
    }

    pub unsafe fn lua_pushstring(l: *mut LuauState, s: *const c_char) {
        unsafe { compat::lua_pushstring(l, s) };
    }

    pub unsafe fn lua_setfield(l: *mut LuauState, idx: c_int, k: *const c_char) {
        unsafe { lua::lua_setfield(l, idx, k) };
    }

    pub unsafe fn lua_setmetatable(l: *mut LuauState, idx: c_int) -> c_int {
        unsafe { lua::lua_setmetatable(l, idx) }
    }

    pub unsafe fn lua_pushvalue(l: *mut LuauState, idx: c_int) {
        unsafe { lua::lua_pushvalue(l, idx) };
    }

    pub unsafe fn lua_absindex(l: *mut LuauState, idx: c_int) -> c_int {
        unsafe { lua::lua_absindex(l, idx) }
    }

    pub unsafe fn lua_gettable(l: *mut LuauState, idx: c_int) {
        unsafe { lua::lua_gettable(l, idx) };
    }

    pub unsafe fn lua_remove(l: *mut LuauState, idx: c_int) {
        unsafe { lua::lua_remove(l, idx) };
    }

    pub unsafe fn luaL_checkstring(l: *mut LuauState, narg: c_int) -> *const c_char {
        unsafe { lauxlib::luaL_checkstring(l, narg) }
    }

    pub unsafe fn lua_pushcfunction(l: *mut LuauState, f: lua::lua_CFunction) {
        unsafe { lua::lua_pushcfunction(l, f) };
    }

    pub unsafe fn lua_settable(l: *mut LuauState, idx: c_int) {
        unsafe { lua::lua_settable(l, idx) };
    }

    pub unsafe fn lua_pushboolean(l: *mut LuauState, b: c_int) {
        unsafe { lua::lua_pushboolean(l, b) };
    }

    pub unsafe fn lua_setglobal(l: *mut LuauState, k: *const c_char) {
        unsafe { lua::lua_setglobal(l, k) };
    }

    pub unsafe fn lua_getglobal(l: *mut LuauState, k: *const c_char) {
        unsafe { lua::lua_getglobal(l, k) };
    }

    pub unsafe fn luaL_newstate() -> *mut LuauState {
        unsafe { lauxlib::luaL_newstate() }
    }

    pub unsafe fn luaL_openlibs(l: *mut LuauState) {
        unsafe { lualib::luaL_openlibs(l) };
    }

    pub unsafe fn lua_close(l: *mut LuauState) {
        unsafe { lua::lua_close(l) };
    }

    pub unsafe fn luaL_loadbuffer(l: *mut LuauState, buff: *const c_char, sz: usize, name: *const c_char) -> c_int {
        unsafe { compat::luaL_loadbuffer(l, buff, sz, name) }
    }

    pub unsafe fn lua_pcall(l: *mut LuauState, nargs: c_int, nresults: c_int, errfunc: c_int) -> c_int {
        unsafe { lua::lua_pcall(l, nargs, nresults, errfunc) }
    }
}

#[cfg(windows)]
mod hires_timer {
    use std::env;
    use std::os::raw::c_uint;

    type MmResult = u32;

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn timeBeginPeriod(uPeriod: c_uint) -> MmResult;
        fn timeEndPeriod(uPeriod: c_uint) -> MmResult;
    }

    pub struct Guard {
        enabled: bool,
    }

    impl Guard {
        pub fn maybe_enable() -> Self {
            let enabled = match env::var_os("NICY_HIRES_TIMER") {
                Some(v) => v != "0",
                None => false,
            };
            if enabled {
                unsafe {
                    timeBeginPeriod(1);
                }
            }

            Self { enabled }
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            if self.enabled {
                unsafe {
                    timeEndPeriod(1);
                }
            }
        }
    }
}

type LuauState = api::LuauState;

unsafe fn push_loadlib_error(l: *mut LuauState, msg: &str) -> c_int {
    let filtered = msg.replace('\0', "?");
    unsafe { api::lua_pushnil(l) };
    unsafe { api::lua_pushlstring(l, filtered.as_ptr() as *const c_char, filtered.as_bytes().len()) };
    2
}

static LOADED_LIBS: OnceLock<Mutex<Vec<Library>>> = OnceLock::new();

fn loaded_libs() -> &'static Mutex<Vec<Library>> {
    LOADED_LIBS.get_or_init(|| Mutex::new(Vec::new()))
}

fn panic_payload_to_string(p: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = p.downcast_ref::<String>() {
        return s.clone();
    }
    "non-string panic payload".to_string()
}

fn log_panic(context: &str, p: Box<dyn std::any::Any + Send>) {
    eprintln!("[NICY PANIC] {}: {}", context, panic_payload_to_string(p));
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

unsafe fn string_from_stack(l: *mut LuauState, idx: c_int) -> String {
    let p = unsafe { api::lua_tostring(l, idx) };
    if p.is_null() {
        return "nil".to_string();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().to_string()
}

unsafe extern "C-unwind" fn nicy_runtime_warn(l: *mut LuauState) -> c_int {
    let top = unsafe { api::lua_gettop(l) };
    if top <= 0 {
        eprintln!("[NICY WARN]");
        return 0;
    }
    let mut parts = Vec::with_capacity(top as usize);
    for i in 1..=top {
        parts.push(unsafe { string_from_stack(l, i) });
    }
    eprintln!("[NICY WARN] {}", parts.join(" "));
    0
}

unsafe fn get_or_create_extension_cache_table(l: *mut LuauState) {
    unsafe { api::lua_getfield(l, api::LUA_REGISTRYINDEX, b"nicy_ext_cache\0".as_ptr() as *const c_char) };
    if unsafe { api::lua_type(l, -1) } != api::LUA_TTABLE {
        unsafe { api::lua_settop(l, -2) };

        unsafe { api::lua_createtable(l, 0, 0) };
        unsafe { api::lua_createtable(l, 0, 1) };
        unsafe { api::lua_pushstring(l, b"v\0".as_ptr() as *const c_char) };
        unsafe { api::lua_setfield(l, -2, b"__mode\0".as_ptr() as *const c_char) };
        unsafe { api::lua_setmetatable(l, -2) };

        unsafe { api::lua_pushvalue(l, -1) };
        unsafe { api::lua_setfield(l, api::LUA_REGISTRYINDEX, b"nicy_ext_cache\0".as_ptr() as *const c_char) };
    }
}

unsafe extern "C-unwind" fn nicy_runtime_loadlib(l: *mut LuauState) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        let path_ptr = api::luaL_checkstring(l, 1);
        if path_ptr.is_null() {
            return Err("invalid path".to_string());
        }

        let path_spec = CStr::from_ptr(path_ptr)
            .to_str()
            .map_err(|_| "invalid path encoding".to_string())?;

        let resolved_path = require_resolver::resolve_loadlib_path(l, path_spec)?;
        let resolved_key = resolved_path.to_string_lossy().to_string();

        get_or_create_extension_cache_table(l);
        let cache_idx = api::lua_absindex(l, -1);
        api::lua_pushlstring(l, resolved_key.as_ptr() as *const c_char, resolved_key.as_bytes().len());
        api::lua_gettable(l, cache_idx);
        if api::lua_type(l, -1) != api::LUA_TNIL {
            api::lua_remove(l, cache_idx);
            return Ok(1);
        }
        api::lua_settop(l, -2);

        let lib = Library::new(&resolved_path)
            .map_err(|e| format!("failed to load library '{}': {}", resolved_path.display(), e))?;

        let init_fn: Symbol<unsafe extern "C-unwind" fn(*mut LuauState) -> c_int> = lib
            .get(b"nicydynamic_init")
            .or_else(|_| lib.get(b"nicydinamic_init"))
            .map_err(|e| format!("missing extension init symbol (nicydynamic_init): {}", e))?;

        let init_res = catch_unwind(AssertUnwindSafe(|| init_fn(l)));
        let res = match init_res {
            Ok(v) => v,
            Err(p) => return Err(format!("extension panic during init: {}", panic_payload_to_string(p))),
        };

        if res != 1 {
            return Err("invalid extension return count (expected 1)".to_string());
        }

        get_or_create_extension_cache_table(l);
        let cache_idx = api::lua_absindex(l, -1);
        api::lua_pushlstring(l, resolved_key.as_ptr() as *const c_char, resolved_key.as_bytes().len());
        api::lua_pushvalue(l, -3);
        api::lua_settable(l, cache_idx);
        api::lua_remove(l, cache_idx);

        loaded_libs()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(lib);
        Ok(1)
    }));

    match result {
        Ok(Ok(n)) => n,
        Ok(Err(msg)) => unsafe { push_loadlib_error(l, &msg) },
        Err(p) => unsafe { push_loadlib_error(l, &format!("runtime panic in runtime.loadlib: {}", panic_payload_to_string(p))) },
    }
}

unsafe extern "C-unwind" fn nicy_runtime_has_jit(l: *mut LuauState) -> c_int {
    let top = unsafe { api::lua_gettop(l) };
    let spec = if top >= 1 {
        let ptr = unsafe { api::luaL_checkstring(l, 1) };
        if ptr.is_null() {
            None
        } else {
            unsafe { CStr::from_ptr(ptr) }.to_str().ok()
        }
    } else {
        None
    };

    let enabled = require_resolver::has_jit(l, spec);
    unsafe { api::lua_pushboolean(l, enabled as c_int) };
    1
}

unsafe fn push_nicy_table(l: *mut LuauState, entry_path: &PathBuf) {
    unsafe { api::lua_createtable(l, 0, 5) };

    unsafe { api::lua_pushstring(l, RUNTIME_VERSION.as_ptr() as *const c_char) };
    unsafe { api::lua_setfield(l, -2, b"version\0".as_ptr() as *const c_char) };

    unsafe { api::lua_pushcfunction(l, nicy_runtime_loadlib) };
    unsafe { api::lua_setfield(l, -2, b"loadlib\0".as_ptr() as *const c_char) };
    unsafe { api::lua_pushcfunction(l, nicy_runtime_has_jit) };
    unsafe { api::lua_setfield(l, -2, b"hasJIT\0".as_ptr() as *const c_char) };

    let entry_file = entry_path.to_string_lossy().to_string();
    unsafe { api::lua_pushlstring(l, entry_file.as_ptr() as *const c_char, entry_file.as_bytes().len()) };
    unsafe { api::lua_setfield(l, -2, b"entry_file\0".as_ptr() as *const c_char) };

    let entry_dir = entry_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    unsafe { api::lua_pushlstring(l, entry_dir.as_ptr() as *const c_char, entry_dir.as_bytes().len()) };
    unsafe { api::lua_setfield(l, -2, b"entry_dir\0".as_ptr() as *const c_char) };

    unsafe { api::lua_setglobal(l, b"runtime\0".as_ptr() as *const c_char) };

    unsafe { api::lua_getglobal(l, b"warn\0".as_ptr() as *const c_char) };
    if unsafe { api::lua_type(l, -1) } == api::LUA_TNIL {
        unsafe { api::lua_settop(l, -2) };
        unsafe { api::lua_pushcfunction(l, nicy_runtime_warn) };
        unsafe { api::lua_setglobal(l, b"warn\0".as_ptr() as *const c_char) };
    } else {
        unsafe { api::lua_settop(l, -2) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nicy_start(path_ptr: *const c_char) {
    if let Err(p) = catch_unwind(AssertUnwindSafe(|| {
        if path_ptr.is_null() {
            eprintln!("[nicyrtdyn] Error: path_ptr is null");
            return;
        }

        #[cfg(windows)]
        let _hires_timer = hires_timer::Guard::maybe_enable();

        let c_str = unsafe { CStr::from_ptr(path_ptr) };
        let path_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("[nicyrtdyn] Error opening file: invalid path encoding");
                return;
            }
        };

        let entry_path = match fs::canonicalize(path_str) {
            Ok(p) => p,
            Err(_) => PathBuf::from(path_str),
        };

        let code = match fs::read_to_string(&entry_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[nicyrtdyn] Error opening file: {}", e);
                return;
            }
        };

        let (entry_native_requested, effective_code) = strip_native_directive(&code);

        unsafe {
            let l = api::luaL_newstate();
            if l.is_null() {
                eprintln!("[nicyrtdyn] Failed to create Luau state");
                return;
            }

            api::luaL_openlibs(l);
            let entry_jit_enabled = entry_native_requested && require_resolver::ensure_codegen_context(l);
            task_scheduler::init(l);
            push_nicy_table(l, &entry_path);
            require_resolver::install_require(l);
            if let Err(e) = require_resolver::init_runtime(l, &entry_path) {
                eprintln!("[NICY REQUIRE ERROR] {}", e);
                api::lua_close(l);
                return;
            }
            if let Err(e) = require_resolver::set_entry_jit(l, entry_jit_enabled) {
                eprintln!("[NICY REQUIRE ERROR] {}", e);
                require_resolver::shutdown_runtime(l);
                api::lua_close(l);
                return;
            }

            let mut chunkname = entry_path.to_string_lossy().as_bytes().to_vec();
            for b in &mut chunkname {
                if *b == 0 {
                    *b = b'?';
                }
            }
            chunkname.push(0);

            let load_status = api::luaL_loadbuffer(
                l,
                effective_code.as_ptr() as *const c_char,
                effective_code.as_bytes().len(),
                chunkname.as_ptr() as *const c_char,
            );
            if load_status != 0 {
                let err = api::lua_tostring(l, -1);
                if !err.is_null() {
                    eprintln!("[LUAU LOAD ERROR] {}", CStr::from_ptr(err).to_string_lossy());
                } else {
                    eprintln!("[LUAU LOAD ERROR] unknown");
                }
                require_resolver::shutdown_runtime(l);
                api::lua_close(l);
                return;
            }

            if entry_jit_enabled {
                mlua_sys::luau::luacodegen::luau_codegen_compile(l, -1);
            }

            if let Err(e) = require_resolver::push_entry_module(l) {
                eprintln!("[NICY REQUIRE ERROR] {}", e);
                require_resolver::shutdown_runtime(l);
                api::lua_close(l);
                return;
            }

            let call_status = api::lua_pcall(l, 0, 0, 0);
            require_resolver::pop_entry_module(l);
            if call_status != 0 {
                let err = api::lua_tostring(l, -1);
                if !err.is_null() {
                    eprintln!("[LUAU ERROR] {}", CStr::from_ptr(err).to_string_lossy());
                } else {
                    eprintln!("[LUAU ERROR] unknown");
                }
                require_resolver::shutdown_runtime(l);
                api::lua_close(l);
                return;
            }

            task_scheduler::run_until_idle(l);

            require_resolver::shutdown_runtime(l);
            api::lua_close(l);
        }
    })) {
        log_panic("nicy_start", p);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nicy_eval(code_ptr: *const c_char) {
    if let Err(p) = catch_unwind(AssertUnwindSafe(|| {
        if code_ptr.is_null() {
            eprintln!("[nicyrtdyn] Error: code_ptr is null");
            return;
        }

        let c_str = unsafe { CStr::from_ptr(code_ptr) };
        let code_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("[nicyrtdyn] Error: invalid code encoding");
                return;
            }
        };

        unsafe {
            let l = api::luaL_newstate();
            if l.is_null() {
                eprintln!("[nicyrtdyn] Failed to create Luau state for eval");
                return;
            }

            api::luaL_openlibs(l);
            push_nicy_table(l, &PathBuf::from("eval"));

            let chunkname = b"eval\0";
            let load_status = api::luaL_loadbuffer(
                l,
                code_str.as_ptr() as *const c_char,
                code_str.as_bytes().len(),
                chunkname.as_ptr() as *const c_char,
            );

            if load_status != 0 {
                let err = api::lua_tostring(l, -1);
                if !err.is_null() {
                    eprintln!("[LUAU EVAL COMPILE ERROR] {}", CStr::from_ptr(err).to_string_lossy());
                } else {
                    eprintln!("[LUAU EVAL COMPILE ERROR] unknown");
                }
                api::lua_close(l);
                return;
            }

            let call_status = api::lua_pcall(l, 0, 0, 0);
            if call_status != 0 {
                let err = api::lua_tostring(l, -1);
                if !err.is_null() {
                    eprintln!("[LUAU EVAL RUNTIME ERROR] {}", CStr::from_ptr(err).to_string_lossy());
                } else {
                    eprintln!("[LUAU EVAL RUNTIME ERROR] unknown");
                }
            }

            api::lua_close(l);
        }
    })) {
        log_panic("nicy_eval", p);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nicy_compile(path_ptr: *const c_char) {
    if let Err(p) = catch_unwind(AssertUnwindSafe(|| {
        if path_ptr.is_null() {
            eprintln!("[nicyrtdyn] Error: path_ptr is null");
            return;
        }

        let c_str = unsafe { CStr::from_ptr(path_ptr) };
        let path_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("[nicyrtdyn] Error: invalid path encoding");
                return;
            }
        };

        let entry_path = match fs::canonicalize(path_str) {
            Ok(p) => p,
            Err(_) => PathBuf::from(path_str),
        };

        let source = match fs::read_to_string(&entry_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[nicyrtdyn] Compile Error reading file: {}", e);
                return;
            }
        };
        let (_, code) = strip_native_directive(&source);

        unsafe {
            let options = mlua_sys::luau::lua_CompileOptions::default();
            let bytecode_result = mlua_sys::luau::luau_compile(
                code.as_bytes(),
                options,
            );

            if bytecode_result.is_empty() {
                eprintln!("[LUAU COMPILE ERROR] Failed to generate bytecode (syntax error?)");
                return;
            }

            let out_path = entry_path.with_extension("luauc");
            if let Err(e) = fs::write(&out_path, &bytecode_result) {
                eprintln!("[LUAU COMPILE ERROR] Failed to save bytecode to {}: {}", out_path.display(), e);
            } else { 
                println!("[NICY] Bytecode successfully compiled to {}", out_path.display());
            }
        }
    })) {
        log_panic("nicy_compile", p);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nicy_version() -> *const c_char {
    RUNTIME_VERSION_LABEL.as_ptr() as *const c_char
}

#[unsafe(no_mangle)]
pub extern "C" fn nicy_luau_version() -> *const c_char {
    static LUAU_VERSION: OnceLock<CString> = OnceLock::new();

    let c_str = LUAU_VERSION.get_or_init(|| {
        unsafe {
            let l = api::luaL_newstate();
            if l.is_null() {
                return CString::new("Luau (Fallback)").unwrap_or_default();
            }

            api::luaL_openlibs(l);

            api::lua_getglobal(l, b"_VERSION\0".as_ptr() as *const c_char);

            let p = api::lua_tostring(l, -1);
            let version_str = if !p.is_null() {
                std::ffi::CStr::from_ptr(p).to_owned()
            } else {
                CString::new("Luau (Unknown)").unwrap_or_default()
            };

            api::lua_close(l);
            
            version_str
        }
    });

    c_str.as_ptr()
}
