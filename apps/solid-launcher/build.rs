//! Puts the Windows icon and version details into the executable itself.
//!
//! Windows asks a program for its own icon: the Start Menu, Explorer, the desktop, the
//! task switcher and a shortcut all read it out of the file. Nothing else can supply it —
//! the icon the running window shows comes from eframe, and the one in Add or remove
//! programs comes from the MSI, so a build without this looked fine in the taskbar and in
//! the tray while the Start Menu showed a blank page.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/solid-launcher.ico");

    // The target, not the host: this also runs when cross-compiling to Windows from
    // somewhere else, and must do nothing when the target is not Windows.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("../../assets/solid-launcher.ico");
    resource.set("FileDescription", "Install, update and open SolidCraft");
    resource.set("ProductName", "SolidLauncher");
    resource.set("OriginalFilename", "solid-launcher.exe");

    match resource.compile() {
        Ok(()) => {}
        // On MSVC the resource compiler ships with the toolchain, so a failure there is a
        // broken build and should stop: that is the target every Windows release is built
        // for. Cross-compiling with the GNU toolchain needs windres, which a machine may
        // not have; say so and carry on rather than making a lint run impossible.
        Err(err) if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") => {
            panic!("could not put the icon into the executable: {err}");
        }
        Err(err) => {
            println!("cargo:warning=no icon was put into the executable ({err})");
        }
    }
}
