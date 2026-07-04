use std::path::Path;

fn main() {
    let src = Path::new("src");

    let sources: &[&str] = &[
        "jaricom.c",
        "jcomapi.c",
        "jdapimin.c",
        "jdapistd.c",
        "jdarith.c",
        "jdatasrc.c",
        "jdcoefct.c",
        "jdcolor.c",
        "jddctmgr.c",
        "jdhuff.c",
        "jdinput.c",
        "jdmainct.c",
        "jdmarker.c",
        "jdmaster.c",
        "jdmerge.c",
        "jdpostct.c",
        "jdsample.c",
        "jdtrans.c",
        "jerror.c",
        "jidctflt.c",
        "jidctfst.c",
        "jidctint.c",
        "jquant1.c",
        "jquant2.c",
        "jutils.c",
        "jmemmgr.c",
        "jmemnobs.c",
    ];

    let mut build = cc::Build::new();
    build
        .include(src)
        .flag_if_supported("-O2")
        .flag_if_supported("-fno-strict-aliasing")
        .flag_if_supported("-Wno-main")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-shift-negative-value")
        .warnings(false);

    for s in sources {
        build.file(src.join(s));
        println!("cargo:rerun-if-changed=src/{s}");
    }

    build.file("cpp/jpeg9c_wrapper.c");
    println!("cargo:rerun-if-changed=cpp/jpeg9c_wrapper.c");

    build.compile("jpeg9c_combined");
}
