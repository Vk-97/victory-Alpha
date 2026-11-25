#[path = "../../crates/victory-shader/agility_build.rs"]
mod agility_build;

fn main() {
    agility_build::configure(true, true);
}
