console.error(
  "Linux updater feed generation is disabled while Linux release assets are deb-only. " +
    "Publish CodexHub_<version>_amd64.deb and CodexHub_<version>_arm64.deb as manual Ubuntu/Debian install packages."
);
process.exit(1);
