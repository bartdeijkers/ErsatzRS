module.exports = {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "scope-enum": [
      2,
      "always",
      [
        "domain",
        "core",
        "ffmpeg",
        "infra",
        "app",
        "web",
        "scanner",
        "docs",
        "ci",
        "deps",
        "release",
      ],
    ],
  },
};
