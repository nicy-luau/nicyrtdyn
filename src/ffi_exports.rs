/*
Copyright (C) 2026 Yanlvl99 | Nicy Luau Runtime Development

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::LuauState;
use mlua_sys::luau::compat;
use mlua_sys::luau::lauxlib;
use mlua_sys::luau::lua;
use std::os::raw::{c_char, c_int, c_void};

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_setfield(l: *mut LuauState, idx: c_int, k: *const c_char) {
    unsafe { lua::lua_setfield(l, idx, k) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_createtable(l: *mut LuauState, narr: c_int, nrec: c_int) {
    unsafe { lua::lua_createtable(l, narr, nrec) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pushstring(l: *mut LuauState, s: *const c_char) {
    unsafe { compat::lua_pushstring(l, s) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pushboolean(l: *mut LuauState, b: c_int) {
    unsafe { lua::lua_pushboolean(l, b) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pushcfunction(l: *mut LuauState, f: lua::lua_CFunction) {
    unsafe { lua::lua_pushcfunction(l, f) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_settop(l: *mut LuauState, idx: c_int) {
    unsafe { lua::lua_settop(l, idx) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_rawseti(l: *mut LuauState, idx: c_int, n: lua::lua_Integer) {
    unsafe { compat::lua_rawseti(l, idx, n) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pushnumber(l: *mut LuauState, n: lua::lua_Number) {
    unsafe { lua::lua_pushnumber(l, n) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_toboolean(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_toboolean(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_tostring(l: *mut LuauState, idx: c_int) -> *const c_char {
    unsafe { lua::lua_tostring(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_luaL_checkstring(l: *mut LuauState, narg: c_int) -> *const c_char {
    unsafe { lauxlib::luaL_checkstring(l, narg) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_luaL_checklstring(l: *mut LuauState, narg: c_int, len: *mut usize) -> *const c_char {
    unsafe { lauxlib::luaL_checklstring(l, narg, len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pushlstring(l: *mut LuauState, s: *const c_char, len: usize) {
    unsafe { compat::lua_pushlstring(l, s, len) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_luaL_error(l: *mut LuauState, msg: *const c_char) -> c_int {
    unsafe { lauxlib::luaL_error(l, msg) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_absindex(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_absindex(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_gettop(l: *mut LuauState) -> c_int {
    unsafe { lua::lua_gettop(l) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pushvalue(l: *mut LuauState, idx: c_int) {
    unsafe { lua::lua_pushvalue(l, idx) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_remove(l: *mut LuauState, idx: c_int) {
    unsafe { lua::lua_remove(l, idx) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_insert(l: *mut LuauState, idx: c_int) {
    unsafe { lua::lua_insert(l, idx) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_type(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_type(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_typename(l: *mut LuauState, tp: c_int) -> *const c_char {
    unsafe { lua::lua_typename(l, tp) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pushnil(l: *mut LuauState) {
    unsafe { lua::lua_pushnil(l) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pushlightuserdata(l: *mut LuauState, p: *mut c_void) {
    unsafe { lua::lua_pushlightuserdata(l, p) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_tolstring(l: *mut LuauState, idx: c_int, len: *mut usize) -> *const c_char {
    unsafe { lua::lua_tolstring(l, idx, len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_getfield(l: *mut LuauState, idx: c_int, k: *const c_char) {
    unsafe { lua::lua_getfield(l, idx, k) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_getglobal(l: *mut LuauState, k: *const c_char) {
    unsafe { lua::lua_getglobal(l, k) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_setglobal(l: *mut LuauState, k: *const c_char) {
    unsafe { lua::lua_setglobal(l, k) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_rawgeti(l: *mut LuauState, idx: c_int, n: lua::lua_Integer) {
    unsafe { compat::lua_rawgeti(l, idx, n) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_newuserdata(l: *mut LuauState, sz: usize) -> *mut c_void {
    unsafe { lua::lua_newuserdata(l, sz) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_touserdata(l: *mut LuauState, idx: c_int) -> *mut c_void {
    unsafe { lua::lua_touserdata(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_getmetatable(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_getmetatable(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_setmetatable(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_setmetatable(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_error(l: *mut LuauState) -> c_int {
    unsafe { lua::lua_error(l) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pcall(l: *mut LuauState, nargs: c_int, nresults: c_int, errfunc: c_int) -> c_int {
    unsafe { lua::lua_pcall(l, nargs, nresults, errfunc) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_call(l: *mut LuauState, nargs: c_int, nresults: c_int) {
    unsafe { lua::lua_call(l, nargs, nresults) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_checkstack(l: *mut LuauState, extra: c_int) -> c_int {
    unsafe { lua::lua_checkstack(l, extra) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_concat(l: *mut LuauState, n: c_int) {
    unsafe { lua::lua_concat(l, n) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_next(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_next(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_gettable(l: *mut LuauState, idx: c_int) {
    unsafe { lua::lua_gettable(l, idx) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_settable(l: *mut LuauState, idx: c_int) {
    unsafe { lua::lua_settable(l, idx) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_rawget(l: *mut LuauState, idx: c_int) {
    unsafe { lua::lua_rawget(l, idx) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_rawset(l: *mut LuauState, idx: c_int) {
    unsafe { lua::lua_rawset(l, idx) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_rawequal(l: *mut LuauState, idx1: c_int, idx2: c_int) -> c_int {
    unsafe { lua::lua_rawequal(l, idx1, idx2) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_isnil(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_isnil(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_isnumber(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_isnumber(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_isstring(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_isstring(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_istable(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_istable(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_isfunction(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_isfunction(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_isuserdata(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_isuserdata(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_isthread(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_isthread(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_isboolean(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_isboolean(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_iscfunction(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { lua::lua_iscfunction(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pushinteger(l: *mut LuauState, n: lua::lua_Integer) {
    unsafe { compat::lua_pushinteger(l, n) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_tointeger(l: *mut LuauState, idx: c_int) -> lua::lua_Integer {
    unsafe { compat::lua_tointeger(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_isinteger(l: *mut LuauState, idx: c_int) -> c_int {
    unsafe { compat::lua_isinteger(l, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_pushcclosure(l: *mut LuauState, f: lua::lua_CFunction, n: c_int) {
    unsafe { lua::lua_pushcclosure(l, f, n) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_newthread(l: *mut LuauState) -> *mut LuauState {
    unsafe { lua::lua_newthread(l) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_resume(l: *mut LuauState, from: *mut LuauState, nargs: c_int, nres: *mut c_int) -> c_int {
    unsafe { compat::lua_resume(l, from, nargs, nres) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_yield(l: *mut LuauState, nresults: c_int) -> c_int {
    unsafe { lua::lua_yield(l, nresults) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_lua_gc(l: *mut LuauState, what: c_int, data: c_int) -> c_int {
    unsafe { lua::lua_gc(l, what, data) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_luaL_ref(l: *mut LuauState, t: c_int) -> c_int {
    unsafe { lauxlib::luaL_ref(l, t) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_luaL_unref(l: *mut LuauState, t: c_int, r: c_int) {
    unsafe { lauxlib::luaL_unref(l, t, r) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn nicy_luaL_checkinteger(l: *mut LuauState, narg: c_int) -> lua::lua_Integer {
    unsafe { compat::luaL_checkinteger(l, narg) }
}
