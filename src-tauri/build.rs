fn main() {
    // Testy na Windows: plik testowy biblioteki nie dostaje manifestu aplikacji, laduje stara
    // comctl32 i nie startuje (STATUS_ENTRYPOINT_NOT_FOUND). Bieg test-windows ustawia
    // LUZIK_MANIFEST_TESTOW=1 i wtedy ten sam manifest co w aplikacji dostaje kazdy plik exe.
    // Wydanie tej zmiennej nie ustawia, wiec instalator buduje sie jak dotad.
    println!("cargo:rerun-if-env-changed=LUZIK_MANIFEST_TESTOW");
    let msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if msvc && std::env::var_os("LUZIK_MANIFEST_TESTOW").is_some() {
        let manifest = std::env::current_dir().unwrap().join("manifest-testow.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        let okna = tauri_build::WindowsAttributes::new_without_app_manifest();
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(okna)).expect("tauri-build");
        return;
    }
    tauri_build::build()
}
