{
  config,
  lib,
  pkgs,
  ...
}:
with lib;
let
  cfg = config.services.taildrop-notifier;
in
{
  options = {
    services.taildrop-notifier = {
      enable = mkEnableOption "Taildrop Notifier";
      user = mkOption {
        type = types.nullOr types.str;
        default = null;
      };
      package = mkOption {
        type = types.package;
        default = pkgs.taildrop-notifier;
      };
    };
  };
  config = mkIf cfg.enable {
    systemd.services.taildrop-notifier = {
      enable = true;
      description = "Taildrop Notifier";
      after = [ "tailscaled.service" ];
      wantedBy = [ "default.target" ];
      path = [ pkgs.pipewire ];
      serviceConfig = {
        Type = "simple";
        Restart = "always";
        ExecStart = "${getExe pkgs.taildrop-notifier} -u ${cfg.user}";
        PrivateTmp = true;
      };
    };
    assertions = [
      {
        assertion = cfg.user != null;
        message = "`services.taildrop-notifier.user` must be set when `services.taildrop-notifier.enable` is true";
      }
      {
        assertion = config.services.tailscale.enable;
        message = "This program requires `services.tailscale.enable` to be enabled.";
      }
    ];
  };
}
