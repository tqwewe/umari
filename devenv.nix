{
  pkgs,
  ...
}:

{
  packages = with pkgs; [
    git
    protobuf # UmaDB
    sqlite
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
    targets = [
      "wasm32-wasip1"
      "wasm32-wasip2"
    ];
  };
}
