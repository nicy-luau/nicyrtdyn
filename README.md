# nicyrtdyn

A dynamic Rust library (`cdylib`) that provides a Luau runtime environment with a custom module resolver and an asynchronous task scheduler.

## Overview

`nicyrtdyn` is designed to be a flexible and high-performance runtime for Luau scripts. It's built in Rust and can be loaded dynamically by a host application. It provides a sandboxed environment for Luau scripts with a custom `require` implementation that supports caching, file fingerprinting, and aliasing.

To enable Luau CodeGen/JIT for a specific file, add `--!native` on the first line of that file (entry or required module).

## Features

- **Embedded Luau:** Comes with a vendored Luau runtime via `mlua-sys`.
- **Dynamic Library:** Built as a `cdylib` for easy dynamic loading.
- **Native Code Integration:** Enables loading of native shared libraries directly from Luau using `runtime.loadlib`. This allows for high-performance extensions written in languages like C, C++, or Rust, which can include anything from JIT-compiled logic to GPU-accelerated code.
- **Custom Module Resolver:** A sophisticated `require()` implementation with:
  - Module caching based on file fingerprints.
  - Automatic cache invalidation.
  - Support for `.luaurc` alias files.
  - Circular dependency detection.
- **Asynchronous Task Scheduler:** A simple cooperative multitasking scheduler for Luau coroutines, with support for `task.spawn`, `task.defer`, `task.delay`, and `task.wait`.
- **FFI:** Exposes a `runtime` global object to Luau for interacting with the host, including a `runtime.loadlib` function for loading other dynamic libraries.
- **High-Resolution Timer:** (Windows-only) Optional high-resolution timer for `task.wait`.

## Luau API

### `runtime` object

A global `runtime` object is available in Luau:

- `runtime.version`: The version of the `nicyrtdyn` library.
- `runtime.hasJIT(path?: string)`: Returns `true` if JIT/CodeGen is active for the given module path. Without argument, it checks the current file.
- `runtime.entry_file`: The path to the main script being executed.
- `runtime.entry_dir`: The directory of the main script.
- `runtime.loadlib(path: string)`: Loads a dynamic library. The path can be relative and use the `@self` alias.

### `task` library

A global `task` library is available for cooperative multitasking:

- `task.spawn(f, ...)`: Spawns a new coroutine.
- `task.defer(f, ...)`: Similar to `task.spawn`.
- `task.delay(seconds, f, ...)`: Spawns a coroutine after a delay.
- `task.wait(seconds)`: Pauses the current coroutine for a given number of seconds.
- `task.cancel(thread|delay_id)`: Cancels a running task.

## Getting Started

### Prerequisites

- Rust 2021 edition or later.

### Building

```bash
cargo build --release
```

The compiled library will be located in `target/release/`.

## Architecture

The library is structured into several modules:

- `lib.rs`: The main entry point, Luau state initialization, and `nicy_start` function.
- `require_resolver.rs`: The custom `require` implementation and module caching logic.
- `task_scheduler.rs`: The asynchronous task scheduler.
- `ffi_exports.rs`: C-ABI compatible functions for FFI.

## 🛠️ Exported C-ABI Methods (Host API)

The `nicyrtdyn` shared library exposes the following `extern "C"` functions. This makes it incredibly easy to embed the Nicy Luau Runtime into any programming language (C#, Python, Node.js, etc.).

* **`void nicy_start(const char* filepath)`** **The Main Engine:** Initializes the complete runtime environment, sets up the task scheduler and custom `require` resolver, and executes the `.luau` script at the specified path.

* **`void nicy_eval(const char* code)`** **The Quick Executor:** Instantly evaluates and executes a raw string of Luau code in an isolated state. Perfect for REPLs, debugging, or on-the-fly execution.

* **`void nicy_compile(const char* filepath)`** **The Bytecode Generator:** Reads the source file, compiles it into highly optimized Luau bytecode, and saves it to disk as a `.luauc` file. *(Note: This does not execute the code).*

* **`const char* nicy_version()`** **Version Info:** Returns a pointer to a null-terminated string containing the Nicy Runtime version (e.g., "Nicy Runtime 0.1.0").

* **`const char* nicy_luau_version()`** **Luau Version:** Returns a pointer to a null-terminated string containing the Luau engine version (e.g., "Luau").

## FFI C-ABI Exports

The following functions are exported with C-ABI compatibility for use in native modules:

```c
// Table operations
void nicy_lua_createtable(LuauState *l, int narr, int nrec);
void nicy_lua_setfield(LuauState *l, int idx, const char *k);
void nicy_lua_getfield(LuauState *l, int idx, const char *k);
void nicy_lua_gettable(LuauState *l, int idx);
void nicy_lua_settable(LuauState *l, int idx);
void nicy_lua_rawget(LuauState *l, int idx);
void nicy_lua_rawset(LuauState *l, int idx);
void nicy_lua_rawgeti(LuauState *l, int idx, lua_Integer n);
void nicy_lua_rawseti(LuauState *l, int idx, lua_Integer n);

// Stack manipulation
void nicy_lua_settop(LuauState *l, int idx);
int nicy_lua_gettop(LuauState *l);
void nicy_lua_pushvalue(LuauState *l, int idx);
void nicy_lua_remove(LuauState *l, int idx);
void nicy_lua_insert(LuauState *l, int idx);
int nicy_lua_absindex(LuauState *l, int idx);
int nicy_lua_checkstack(LuauState *l, int extra);

// Pushing values to the stack
void nicy_lua_pushnil(LuauState *l);
void nicy_lua_pushboolean(LuauState *l, int b);
void nicy_lua_pushnumber(LuauState *l, lua_Number n);
void nicy_lua_pushinteger(LuauState *l, lua_Integer n);
void nicy_lua_pushstring(LuauState *l, const char *s);
void nicy_lua_pushlstring(LuauState *l, const char *s, size_t len);
void nicy_lua_pushcfunction(LuauState *l, lua_CFunction f);
void nicy_lua_pushcclosure(LuauState *l, lua_CFunction f, int n);
void nicy_lua_pushlightuserdata(LuauState *l, void *p);

// Type checking and retrieval
int nicy_lua_type(LuauState *l, int idx);
const char *nicy_lua_typename(LuauState *l, int tp);
int nicy_lua_isnil(LuauState *l, int idx);
int nicy_lua_isnumber(LuauState *l, int idx);
int nicy_lua_isstring(LuauState *l, int idx);
int nicy_lua_istable(LuauState *l, int idx);
int nicy_lua_isfunction(LuauState *l, int idx);
int nicy_lua_isuserdata(LuauState *l, int idx);
int nicy_lua_isthread(LuauState *l, int idx);
int nicy_lua_isboolean(LuauState *l, int idx);
int nicy_lua_iscfunction(LuauState *l, int idx);
int nicy_lua_isinteger(LuauState *l, int idx);

// Converting stack values
int nicy_lua_toboolean(LuauState *l, int idx);
const char *nicy_lua_tostring(LuauState *l, int idx);
const char *nicy_lua_tolstring(LuauState *l, int idx, size_t *len);
lua_Integer nicy_lua_tointeger(LuauState *l, int idx);
void *nicy_lua_touserdata(LuauState *l, int idx);

// Checking arguments
const char *nicy_luaL_checkstring(LuauState *l, int narg);
const char *nicy_luaL_checklstring(LuauState *l, int narg, size_t *len);
lua_Integer nicy_luaL_checkinteger(LuauState *l, int narg);

// Globals
void nicy_lua_getglobal(LuauState *l, const char *k);
void nicy_lua_setglobal(LuauState *l, const char *k);

// Metatables
int nicy_lua_getmetatable(LuauState *l, int idx);
int nicy_lua_setmetatable(LuauState *l, int idx);

// Function calls
void nicy_lua_call(LuauState *l, int nargs, int nresults);
int nicy_lua_pcall(LuauState *l, int nargs, int nresults, int errfunc);

// Coroutines
LuauState *nicy_lua_newthread(LuauState *l);
int nicy_lua_resume(LuauState *l, LuauState *from, int nargs, int *nres);
int nicy_lua_yield(LuauState *l, int nresults);

// Userdata
void *nicy_lua_newuserdata(LuauState *l, size_t sz);

// Miscellaneous
void nicy_lua_concat(LuauState *l, int n);
int nicy_lua_next(LuauState *l, int idx);
int nicy_lua_rawequal(LuauState *l, int idx1, int idx2);
int nicy_lua_gc(LuauState *l, int what, int data);

// Error handling
int nicy_lua_error(LuauState *l);
int nicy_luaL_error(LuauState *l, const char *msg);

// References
int nicy_luaL_ref(LuauState *l, int t);
void nicy_luaL_unref(LuauState *l, int t, int r);
```

## License

This project is licensed under the Mozilla Public License 2.0. See the [LICENSE](LICENSE) file for details.
