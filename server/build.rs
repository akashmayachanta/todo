// Gives the Windows exe its icon and manifest. Does nothing on other systems.
fn main() {
    for file in ["resources.rc", "tasks-server.manifest", "icons/icon.ico"] {
        println!("cargo:rerun-if-changed={file}");
    }
    embed_resource::compile("resources.rc", embed_resource::NONE).manifest_required().unwrap();
}
