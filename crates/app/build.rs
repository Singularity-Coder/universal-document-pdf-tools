fn main() {
    // Embed the app icon into the Windows executable.
    #[cfg(windows)]
    {
        let ico = std::path::Path::new("../../assets/icon.ico");
        if ico.exists() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon(ico.to_str().unwrap());
            res.set("ProductName", "Universal Document PDF Tools");
            res.set("FileDescription", "Universal Document PDF Tools - offline document tools");
            let _ = res.compile();
        }
    }
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
}
