//! Prints the diagram JSON Schema to stdout. Used by `gen-schema`.

fn main() {
    let schema = infraplot_model::json_schema();
    println!(
        "{}",
        serde_json::to_string_pretty(&schema).expect("schema serialises")
    );
}
