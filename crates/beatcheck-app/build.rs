fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=../../assets/brand/beatcheck.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/brand/beatcheck.ico");
        res.set("ProductName", "BeatCheck");
        res.set("FileDescription", "BeatCheck — Cut on the beat. Every frame.");
        if let Err(err) = res.compile() {
            println!("cargo:warning=Could not embed Windows icon resource: {err}");
        }
    }
}
