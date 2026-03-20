/*
Copyright (C) 2026 Yanlvl99 | Nicy Luau Runtime Development

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use libloading::{Library, Symbol};
use mlua_sys::luau::compat;
use mlua_sys::luau::lauxlib;
use mlua_sys::luau::lua;
use mlua_sys::luau::lualib;
use std::ffi::CStr;
use std::fs;
use std::os::raw::{c_char, c_int};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

mod ffi_exports;
mod require_resolver;
mod task_scheduler;

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

type LuauState = lua::lua_State;

unsafe fn push_loadlib_error(l: *mut LuauState, msg: &str) -> c_int {
    let filtered = msg.replace('\0', "?");
    unsafe { lua::lua_pushnil(l) };
    unsafe { compat::lua_pushlstring(l, filtered.as_ptr() as *const c_char, filtered.as_bytes().len()) };
    2
}

static LOADED_LIBS: OnceLock<Mutex<Vec<Library>>> = OnceLock::new();

fn loaded_libs() -> &'static Mutex<Vec<Library>> {
    LOADED_LIBS.get_or_init(|| Mutex::new(Vec::new()))
}

unsafe fn string_from_stack(l: *mut LuauState, idx: c_int) -> String {
    let p = unsafe { lua::lua_tostring(l, idx) };
    if p.is_null() {
        return "nil".to_string();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().to_string()
}

unsafe extern "C-unwind" fn nicy_runtime_warn(l: *mut LuauState) -> c_int {
    let top = unsafe { lua::lua_gettop(l) };
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
    unsafe { lua::lua_getfield(l, lua::LUA_REGISTRYINDEX, b"nicy_ext_cache\0".as_ptr() as *const c_char) };
    if unsafe { lua::lua_type(l, -1) } != lua::LUA_TTABLE {
        unsafe { lua::lua_settop(l, -2) };

        unsafe { lua::lua_createtable(l, 0, 0) };
        unsafe { lua::lua_createtable(l, 0, 1) };
        unsafe { compat::lua_pushstring(l, b"v\0".as_ptr() as *const c_char) };
        unsafe { lua::lua_setfield(l, -2, b"__mode\0".as_ptr() as *const c_char) };
        unsafe { lua::lua_setmetatable(l, -2) };

        unsafe { lua::lua_pushvalue(l, -1) };
        unsafe { lua::lua_setfield(l, lua::LUA_REGISTRYINDEX, b"nicy_ext_cache\0".as_ptr() as *const c_char) };
    }
}

unsafe extern "C-unwind" fn nicy_runtime_loadlib(l: *mut LuauState) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        let path_ptr = lauxlib::luaL_checkstring(l, 1);
        if path_ptr.is_null() {
            return Err("invalid path".to_string());
        }

        let path_spec = CStr::from_ptr(path_ptr)
            .to_str()
            .map_err(|_| "invalid path encoding".to_string())?;

        let resolved_path = require_resolver::resolve_loadlib_path(l, path_spec)?;
        let resolved_key = resolved_path.to_string_lossy().to_string();

        get_or_create_extension_cache_table(l);
        let cache_idx = lua::lua_absindex(l, -1);
        compat::lua_pushlstring(l, resolved_key.as_ptr() as *const c_char, resolved_key.as_bytes().len());
        lua::lua_gettable(l, cache_idx);
        if lua::lua_type(l, -1) != lua::LUA_TNIL {
            lua::lua_remove(l, cache_idx);
            return Ok(1);
        }
        lua::lua_settop(l, -2);

        let lib = Library::new(&resolved_path)
            .map_err(|e| format!("failed to load library '{}': {}", resolved_path.display(), e))?;

        let init_fn: Symbol<unsafe extern "C-unwind" fn(*mut LuauState) -> c_int> = lib
            .get(b"nicydinamic_init")
            .map_err(|e| format!("missing nicydinamic_init: {}", e))?;

        let init_res = catch_unwind(AssertUnwindSafe(|| init_fn(l)));
        let res = match init_res {
            Ok(v) => v,
            Err(_) => return Err("extension panic during nicydinamic_init".to_string()),
        };

        if res != 1 {
            return Err("invalid extension return count (expected 1)".to_string());
        }

        get_or_create_extension_cache_table(l);
        let cache_idx = lua::lua_absindex(l, -1);
        compat::lua_pushlstring(l, resolved_key.as_ptr() as *const c_char, resolved_key.as_bytes().len());
        lua::lua_pushvalue(l, -3);
        lua::lua_settable(l, cache_idx);
        lua::lua_remove(l, cache_idx);

        loaded_libs()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(lib);
        Ok(1)
    }));

    match result {
        Ok(Ok(n)) => n,
        Ok(Err(msg)) => unsafe { push_loadlib_error(l, &msg) },
        Err(_) => unsafe { push_loadlib_error(l, "runtime panic in runtime.loadlib") },
    }
}

unsafe fn push_nicy_table(l: *mut LuauState, native_enabled: bool, entry_path: &PathBuf) {
    unsafe { lua::lua_createtable(l, 0, 3) };

    unsafe { compat::lua_pushstring(l, b"0.0.1\0".as_ptr() as *const c_char) };
    unsafe { lua::lua_setfield(l, -2, b"version\0".as_ptr() as *const c_char) };

    unsafe { lua::lua_pushboolean(l, native_enabled as c_int) };
    unsafe { lua::lua_setfield(l, -2, b"native_enabled\0".as_ptr() as *const c_char) };

    unsafe { lua::lua_createtable(l, 0, 3) };
    unsafe { lua::lua_pushcfunction(l, nicy_runtime_loadlib) };
    unsafe { lua::lua_setfield(l, -2, b"loadlib\0".as_ptr() as *const c_char) };
    let entry_file = entry_path.to_string_lossy().to_string();
    unsafe { compat::lua_pushlstring(l, entry_file.as_ptr() as *const c_char, entry_file.as_bytes().len()) };
    unsafe { lua::lua_setfield(l, -2, b"entry_file\0".as_ptr() as *const c_char) };
    let entry_dir = entry_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    unsafe { compat::lua_pushlstring(l, entry_dir.as_ptr() as *const c_char, entry_dir.as_bytes().len()) };
    unsafe { lua::lua_setfield(l, -2, b"entry_dir\0".as_ptr() as *const c_char) };
    unsafe { lua::lua_setfield(l, -2, b"runtime\0".as_ptr() as *const c_char) };

    unsafe { lua::lua_setglobal(l, b"runtime\0".as_ptr() as *const c_char) };

    unsafe { lua::lua_getglobal(l, b"warn\0".as_ptr() as *const c_char) };
    if unsafe { lua::lua_type(l, -1) } == lua::LUA_TNIL {
        unsafe { lua::lua_settop(l, -2) };
        unsafe { lua::lua_pushcfunction(l, nicy_runtime_warn) };
        unsafe { lua::lua_setglobal(l, b"warn\0".as_ptr() as *const c_char) };
    } else {
        unsafe { lua::lua_settop(l, -2) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nicy_start(path_ptr: *const c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
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

        let first_line = code.lines().next().unwrap_or("");
        let use_native = first_line.trim().starts_with("!native");

        unsafe {
            let l = lauxlib::luaL_newstate();
            if l.is_null() {
                eprintln!("[nicyrtdyn] Failed to create Luau state");
                return;
            }

            lualib::luaL_openlibs(l);
            task_scheduler::init(l);
            push_nicy_table(l, use_native, &entry_path);
            require_resolver::install_require(l);
            if let Err(e) = require_resolver::init_runtime(l, &entry_path) {
                eprintln!("[NICY REQUIRE ERROR] {}", e);
                lua::lua_close(l);
                return;
            }

            let mut chunkname = entry_path.to_string_lossy().as_bytes().to_vec();
            for b in &mut chunkname {
                if *b == 0 {
                    *b = b'?';
                }
            }
            chunkname.push(0);

            let load_status = compat::luaL_loadbuffer(
                l,
                code.as_ptr() as *const c_char,
                code.as_bytes().len(),
                chunkname.as_ptr() as *const c_char,
            );
            if load_status != 0 {
                let err = lua::lua_tostring(l, -1);
                if !err.is_null() {
                    eprintln!("[LUAU LOAD ERROR] {}", CStr::from_ptr(err).to_string_lossy());
                } else {
                    eprintln!("[LUAU LOAD ERROR] unknown");
                }
                require_resolver::shutdown_runtime(l);
                lua::lua_close(l);
                return;
            }

            if let Err(e) = require_resolver::push_entry_module(l) {
                eprintln!("[NICY REQUIRE ERROR] {}", e);
                require_resolver::shutdown_runtime(l);
                lua::lua_close(l);
                return;
            }

            let call_status = lua::lua_pcall(l, 0, 0, 0);
            require_resolver::pop_entry_module(l);
            if call_status != 0 {
                let err = lua::lua_tostring(l, -1);
                if !err.is_null() {
                    eprintln!("[LUAU ERROR] {}", CStr::from_ptr(err).to_string_lossy());
                } else {
                    eprintln!("[LUAU ERROR] unknown");
                }
                require_resolver::shutdown_runtime(l);
                lua::lua_close(l);
                return;
            }

            task_scheduler::run_until_idle(l);

            require_resolver::shutdown_runtime(l);
            lua::lua_close(l);
        }
    }));
}
