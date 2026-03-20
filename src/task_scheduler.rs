/*
Copyright (C) 2026 Yanlvl99 | Nicy Luau Runtime Development

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use mlua_sys::luau::compat;
use mlua_sys::luau::lauxlib;
use mlua_sys::luau::lua;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
#[cfg(windows)]
use std::os::raw::c_void;
use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(windows)]
type WinHandle = *mut c_void;

#[cfg(windows)]
type WinBool = i32;

#[cfg(windows)]
type WinDword = u32;

#[cfg(windows)]
type WinLong = i32;

#[cfg(windows)]
type WinWstr = *const u16;

#[cfg(windows)]
unsafe extern "system" {
    fn CreateWaitableTimerW(lpTimerAttributes: *mut c_void, bManualReset: WinBool, lpTimerName: WinWstr) -> WinHandle;
    fn SetWaitableTimer(
        hTimer: WinHandle,
        lpDueTime: *const i64,
        lPeriod: WinLong,
        pfnCompletionRoutine: *mut c_void,
        lpArgToCompletionRoutine: *mut c_void,
        fResume: WinBool,
    ) -> WinBool;
    fn WaitForSingleObject(hHandle: WinHandle, dwMilliseconds: WinDword) -> WinDword;
}

#[cfg(windows)]
const WIN_INFINITE: WinDword = 0xFFFF_FFFF;

#[cfg(windows)]
const WIN_WAIT_OBJECT_0: WinDword = 0;

#[cfg(windows)]
static WIN_WAITABLE_TIMER: OnceLock<usize> = OnceLock::new();

#[cfg(windows)]
fn win_waitable_timer() -> Option<WinHandle> {
    let h = *WIN_WAITABLE_TIMER
        .get_or_init(|| unsafe { CreateWaitableTimerW(std::ptr::null_mut(), 0, std::ptr::null()) } as usize);
    if h == 0 {
        None
    } else {
        Some(h as WinHandle)
    }
}

type LuauState = lua::lua_State;

enum TaskKey {
    ThreadRef(c_int),
    DelayId(u64),
}

struct ScheduledTask {
    due: Instant,
    key: TaskKey,
}

impl PartialEq for ScheduledTask {
    fn eq(&self, other: &Self) -> bool {
        self.due.eq(&other.due)
    }
}

impl Eq for ScheduledTask {}

impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.due.cmp(&other.due)
    }
}

struct WaitInfo {
    start: Instant,
    #[allow(dead_code)]
    due: Instant,
}

struct Scheduler {
    next_delay_id: u64,
    ready: VecDeque<c_int>,
    timers: BinaryHeap<Reverse<ScheduledTask>>,
    waits: HashMap<c_int, WaitInfo>,
    delay_threads: HashMap<u64, c_int>,
    thread_refs: HashMap<usize, c_int>,
    canceled_threads: HashMap<c_int, ()>,
    canceled_delays: HashMap<u64, ()>,
}

impl Scheduler {
    fn new() -> Self {
        Self {
            next_delay_id: 1,
            ready: VecDeque::new(),
            timers: BinaryHeap::new(),
            waits: HashMap::new(),
            delay_threads: HashMap::new(),
            thread_refs: HashMap::new(),
            canceled_threads: HashMap::new(),
            canceled_delays: HashMap::new(),
        }
    }

    fn has_work(&self) -> bool {
        !self.ready.is_empty() || !self.timers.is_empty()
    }

    fn next_due(&self) -> Option<Instant> {
        self.timers.peek().map(|t| t.0.due)
    }

    fn pop_due(&mut self, now: Instant) -> Vec<c_int> {
        let mut out = Vec::new();
        while let Some(Reverse(top)) = self.timers.peek() {
            if top.due > now {
                break;
            }
            let Some(Reverse(task)) = self.timers.pop() else {
                break;
            };
            match task.key {
                TaskKey::ThreadRef(r) => {
                    if self.canceled_threads.remove(&r).is_none() {
                        out.push(r);
                    } else {
                        self.waits.remove(&r);
                        self.thread_refs.retain(|_, rr| *rr != r);
                        unsafe { lauxlib_unref_current_state(r) };
                    }
                }
                TaskKey::DelayId(id) => {
                    if self.canceled_delays.remove(&id).is_none() {
                        if let Some(r) = self.delay_threads.remove(&id) {
                            out.push(r);
                        }
                    } else {
                        if let Some(r) = self.delay_threads.remove(&id) {
                            self.waits.remove(&r);
                            self.thread_refs.retain(|_, rr| *rr != r);
                            unsafe { lauxlib_unref_current_state(r) };
                        }
                    }
                }
            }
        }
        out
    }
}

static SCHED: OnceLock<Mutex<Scheduler>> = OnceLock::new();

fn sched() -> &'static Mutex<Scheduler> {
    SCHED.get_or_init(|| Mutex::new(Scheduler::new()))
}

static CURRENT_L: AtomicUsize = AtomicUsize::new(0);

unsafe fn lauxlib_unref_current_state(r: c_int) {
    let l = CURRENT_L.load(Ordering::Relaxed) as *mut LuauState;
    if !l.is_null() {
        unsafe { lauxlib::luaL_unref(l, lua::LUA_REGISTRYINDEX, r) };
    }
}

fn duration_from_seconds(secs: f64) -> Duration {
    if !secs.is_finite() || secs <= 0.0 {
        return Duration::from_secs(0);
    }
    let capped = secs.min(60.0 * 60.0 * 24.0 * 365.0 * 10.0);
    Duration::from_secs_f64(capped)
}

unsafe fn raise_panic_as_lua_error(l: *mut LuauState) -> c_int {
    unsafe { compat::lua_pushstring(l, b"task: panic\0".as_ptr() as *const _) };
    unsafe { lua::lua_error(l) }
}

unsafe fn with_lua_panic_guard(l: *mut LuauState, f: impl FnOnce() -> c_int) -> c_int {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => unsafe { raise_panic_as_lua_error(l) },
    }
}

unsafe extern "C-unwind" fn task_spawn(l: *mut LuauState) -> c_int {
    unsafe {
        with_lua_panic_guard(l, || {
            let nargs = lua::lua_gettop(l);
            lauxlib::luaL_checktype(l, 1, lua::LUA_TFUNCTION);

            let th = lua::lua_newthread(l);

            lua::lua_pushvalue(l, 1);
            lua::lua_xmove(l, th, 1);
            for i in 2..=nargs {
                lua::lua_pushvalue(l, i);
                lua::lua_xmove(l, th, 1);
            }

            lua::lua_pushvalue(l, -1);
            let thread_ref = lauxlib::luaL_ref(l, lua::LUA_REGISTRYINDEX);

            {
                let mut s = sched().lock().unwrap_or_else(|e| e.into_inner());
                s.ready.push_back(thread_ref);
                s.thread_refs.insert(th as usize, thread_ref);
            }

            if cfg!(debug_assertions) {
                eprintln!("[task.spawn] queued ref={}", thread_ref);
            }

            1
        })
    }
}

unsafe extern "C-unwind" fn task_defer(l: *mut LuauState) -> c_int {
    unsafe { task_spawn(l) }
}

unsafe extern "C-unwind" fn task_delay(l: *mut LuauState) -> c_int {
    catch_unwind(|| unsafe {
        with_lua_panic_guard(l, || {
            let nargs = lua::lua_gettop(l);
            let secs = lauxlib::luaL_checknumber(l, 1);
            lauxlib::luaL_checktype(l, 2, lua::LUA_TFUNCTION);

            let th = lua::lua_newthread(l);

            lua::lua_pushvalue(l, 2);
            lua::lua_xmove(l, th, 1);
            for i in 3..=nargs {
                lua::lua_pushvalue(l, i);
                lua::lua_xmove(l, th, 1);
            }

            lua::lua_pushvalue(l, -1);
            let thread_ref = lauxlib::luaL_ref(l, lua::LUA_REGISTRYINDEX);

            let now = Instant::now();
            let dur = duration_from_seconds(secs as f64);
            let due = now + dur;

            let id = {
                let mut s = sched().lock().unwrap_or_else(|e| e.into_inner());
                let id = s.next_delay_id;
                s.next_delay_id = s.next_delay_id.wrapping_add(1).max(1);
                s.delay_threads.insert(id, thread_ref);
                s.thread_refs.insert(th as usize, thread_ref);
                s.timers.push(Reverse(ScheduledTask { due, key: TaskKey::DelayId(id) }));
                id
            };

            if cfg!(debug_assertions) {
                eprintln!("[task.delay] queued id={} ref={}", id, thread_ref);
            }

            lua::lua_pushnumber(l, id as f64);
            1
        })
    }).unwrap_or_else(|_| 0)
}

unsafe extern "C-unwind" fn task_wait(l: *mut LuauState) -> c_int {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        with_lua_panic_guard(l, || {
            let secs = if lua::lua_gettop(l) >= 1 {
                lauxlib::luaL_checknumber(l, 1)
            } else {
                0.0
            };

            let dur = duration_from_seconds(secs);

            let is_main = lua::lua_pushthread(l) != 0;
            lua::lua_settop(l, -2);
            if is_main {
                let start = Instant::now();
                let target = start + dur;

                while Instant::now() < target {
                    let mut ready_now = Vec::new();
                    let next_due = {
                        let mut s = sched().lock().unwrap_or_else(|e| e.into_inner());

                        while let Some(r) = s.ready.pop_front() {
                            ready_now.push(r);
                        }

                        let due = s.pop_due(Instant::now());
                        ready_now.extend(due);

                        if !ready_now.is_empty() {
                            None
                        } else {
                            Some(s.next_due().unwrap_or(target).min(target))
                        }
                    };

                    for r in ready_now {
                        let _ = resume_thread(l, r);
                    }

                    if let Some(next) = next_due {
                        if next > Instant::now() {
                            sleep_until(next);
                        }
                    }
                }

                lua::lua_pushnumber(l, Instant::now().duration_since(start).as_secs_f64());
                return 1;
            }

            let _thread_ref = {
                let mut s = sched().lock().unwrap_or_else(|e| e.into_inner());
                let Some(thread_ref) = s.thread_refs.get(&(l as usize)).copied() else {
                    lua::lua_pushnumber(l, 0.0);
                    return 1;
                };
                let now = Instant::now();
                s.waits.insert(
                    thread_ref,
                    WaitInfo {
                        start: now,
                        due: now + dur,
                    },
                );
                s.timers.push(Reverse(ScheduledTask {
                    due: now + dur,
                    key: TaskKey::ThreadRef(thread_ref),
                }));
                thread_ref
            };

            lua::lua_yield(l, 0)
        })
    }))
    .unwrap_or_else(|_| 0)
}

unsafe extern "C-unwind" fn task_cancel(l: *mut LuauState) -> c_int {
    unsafe {
        with_lua_panic_guard(l, || {
            if lua::lua_gettop(l) < 1 {
                lua::lua_pushboolean(l, 0);
                return 1;
            }

            let t = lua::lua_type(l, 1);
            if t == lua::LUA_TTHREAD {
                let th = lua::lua_tothread(l, 1);
                if th.is_null() {
                    lua::lua_pushboolean(l, 0);
                    return 1;
                }

                let thread_ref = {
                    sched()
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .thread_refs
                        .get(&(th as usize))
                        .copied()
                };

                if let Some(r) = thread_ref {
                    let mut s = sched().lock().unwrap_or_else(|e| e.into_inner());
                    s.canceled_threads.insert(r, ());
                    lua::lua_pushboolean(l, 1);
                } else {
                    lua::lua_pushboolean(l, 0);
                }

                return 1;
            }

            if t == lua::LUA_TNUMBER {
                let id = compat::lua_tointeger(l, 1) as i64;
                if id <= 0 {
                    lua::lua_pushboolean(l, 0);
                    return 1;
                }
                {
                    let mut s = sched().lock().unwrap_or_else(|e| e.into_inner());
                    s.canceled_delays.insert(id as u64, ());
                }
                lua::lua_pushboolean(l, 1);
                return 1;
            }

            lua::lua_pushboolean(l, 0);
            1
        })
    }
}

pub fn init(l: *mut LuauState) {
    CURRENT_L.store(l as usize, Ordering::Relaxed);

    unsafe { lua::lua_createtable(l, 0, 5) };

    unsafe { lua::lua_pushcfunction(l, task_spawn) };
    unsafe { lua::lua_setfield(l, -2, b"spawn\0".as_ptr() as *const _) };

    unsafe { lua::lua_pushcfunction(l, task_defer) };
    unsafe { lua::lua_setfield(l, -2, b"defer\0".as_ptr() as *const _) };

    unsafe { lua::lua_pushcfunction(l, task_delay) };
    unsafe { lua::lua_setfield(l, -2, b"delay\0".as_ptr() as *const _) };

    unsafe { lua::lua_pushcfunction(l, task_wait) };
    unsafe { lua::lua_setfield(l, -2, b"wait\0".as_ptr() as *const _) };

    unsafe { lua::lua_pushcfunction(l, task_cancel) };
    unsafe { lua::lua_setfield(l, -2, b"cancel\0".as_ptr() as *const _) };

    unsafe { lua::lua_setglobal(l, b"task\0".as_ptr() as *const _) };
}

fn sleep_until(deadline: Instant) {
    let now = Instant::now();
    if deadline <= now {
        std::thread::yield_now();
        return;
    }

    let dur = deadline - now;

    #[cfg(windows)]
    {
        if sleep_for_waitable_timer(dur) {
            return;
        }
    }

    std::thread::sleep(dur);
}

#[cfg(windows)]
fn sleep_for_waitable_timer(dur: Duration) -> bool {
    if dur.is_zero() {
        return true;
    }

    let Some(h) = win_waitable_timer() else { return false };

    let mut due: i64 = -((dur.as_nanos() / 100) as i64);
    if due == 0 {
        due = -1;
    }

    let ok = unsafe {
        SetWaitableTimer(
            h,
            &due as *const i64,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        return false;
    }

    let w = unsafe { WaitForSingleObject(h, WIN_INFINITE) };
    w == WIN_WAIT_OBJECT_0
}

unsafe fn resume_thread(l: *mut LuauState, thread_ref: c_int) -> Result<bool, ()> {
    unsafe { compat::lua_rawgeti(l, lua::LUA_REGISTRYINDEX, thread_ref as lua::lua_Integer) };
    let th = unsafe { lua::lua_tothread(l, -1) };
    if th.is_null() {
        unsafe { lua::lua_settop(l, -2) };
        unsafe { lauxlib::luaL_unref(l, lua::LUA_REGISTRYINDEX, thread_ref) };
        return Err(());
    }

    let elapsed = {
        let mut s = sched().lock().unwrap_or_else(|e| e.into_inner());
        s.waits.remove(&thread_ref).map(|w| Instant::now().duration_since(w.start).as_secs_f64())
    };

    unsafe { lua::lua_settop(l, -2) };

    let nargs = if let Some(dt) = elapsed {
        unsafe { lua::lua_pushnumber(th, dt) };
        1
    } else {
        0
    };
    let mut nres: c_int = 0;
    let st = unsafe { compat::lua_resume(th, l, nargs, &mut nres as *mut c_int) };

    if st == 0 {
        {
            let mut s = sched().lock().unwrap_or_else(|e| e.into_inner());
            s.thread_refs.remove(&(th as usize));
            s.waits.remove(&thread_ref);
        }
        unsafe { lauxlib::luaL_unref(l, lua::LUA_REGISTRYINDEX, thread_ref) };
        Ok(true)
    } else if st == lua::LUA_YIELD {
        Ok(false)
    } else {
        let err = unsafe { lua::lua_tostring(th, -1) };
        if !err.is_null() {
            eprintln!("[TASK ERROR] {}", unsafe { std::ffi::CStr::from_ptr(err) }.to_string_lossy());
        } else {
            eprintln!("[TASK ERROR] unknown");
        }
        {
            let mut s = sched().lock().unwrap_or_else(|e| e.into_inner());
            s.thread_refs.remove(&(th as usize));
            s.waits.remove(&thread_ref);
        }
        unsafe { lauxlib::luaL_unref(l, lua::LUA_REGISTRYINDEX, thread_ref) };
        Ok(true)
    }
}

pub fn run_until_idle(l: *mut LuauState) {
    loop {
        let mut ready_now = Vec::new();
        {
            let mut s = sched().lock().unwrap_or_else(|e| e.into_inner());
            while let Some(r) = s.ready.pop_front() {
                ready_now.push(r);
            }
            let now = Instant::now();
            ready_now.extend(s.pop_due(now));

            if ready_now.is_empty() {
                if !s.has_work() {
                    break;
                }
                if let Some(next) = s.next_due() {
                    drop(s);
                    sleep_until(next);
                    continue;
                }
            }
        }

        if cfg!(debug_assertions) && !ready_now.is_empty() {
            eprintln!("[task] resuming {} tasks", ready_now.len());
        }

        for r in ready_now {
            let _ = unsafe { resume_thread(l, r) };
        }
    }
}
