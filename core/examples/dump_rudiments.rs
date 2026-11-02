//! Prints the rudiment catalogue as JSON, for the manual (see `docs/manual/rudiments.json`).

fn main() {
    print!("{}", roll_model_core::rudiments::catalogue_json());
}
