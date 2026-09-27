use std::env;

fn main() {
    println!("cargo:rerun-if-changed=c/sqlite-vec.c");
    println!("cargo:rerun-if-changed=c/sqlite-vec.h");

    let mut build = cc::Build::new();
    build.file("c/sqlite-vec.c");
    build.include("c");

    // Retrieve sqlite3.h include path provided by libsqlite3-sys
    if let Ok(sqlite_include) = env::var("DEP_SQLITE3_INCLUDE") {
        build.include(&sqlite_include);
    }

    // Static linking defines: direct integration into core SQLite engine
    build.define("SQLITE_CORE", Some("1"));
    build.define("SQLITE_VEC_STATIC", Some("1"));

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target = env::var("TARGET").unwrap_or_default();

    if target_env == "msvc" || target.contains("msvc") {
        build.flag_if_supported("/O2");
        build.flag_if_supported("/fp:fast");
        build.flag_if_supported("/arch:AVX2");
        build.flag_if_supported("/utf-8");
        // DO NOT define SQLITE_VEC_ENABLE_AVX on MSVC:
        // causes syntax errors with GCC __attribute__ and 0xc0000005 GPF crashes.
        build.warnings(false);
    } else {
        build.flag_if_supported("-O3");
        build.flag_if_supported("-ffast-math");
        build.flag_if_supported("-mavx2");
        build.flag_if_supported("-mfma");
        build.warnings(false);
    }

    build.compile("sqlite_vec");
}
