fn main() {
    println!("cargo:rerun-if-changed=data/midi.xml");
    println!("cargo:rerun-if-changed=data/midi.xsd");
}
