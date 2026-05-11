wit_bindgen::generate!({
    world: "crypto",
    path: "wit/crypto",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    generate_unused_types: true,
    pub_export_macro: true,
});
