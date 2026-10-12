//! Windows : icône et informations de version de l'exécutable.

fn main() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icons/icon.ico")
            .set("ProductName", "Cryptonaute")
            .set("FileDescription", "Cryptonaute")
            .set("CompanyName", "Cryptonaute")
            .set("LegalCopyright", "Cryptonaute contributors — licence MIT");
        if let Err(e) = res.compile() {
            println!("cargo:warning=ressources Windows non intégrées : {e}");
        }
    }
}
