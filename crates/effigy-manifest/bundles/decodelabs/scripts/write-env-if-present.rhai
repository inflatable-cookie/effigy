let template = "infra/dev/bootstrap/app.env";
let target = ".env";

if !fs::exists(template) {
    log("[info] bootstrap .env template not present at " + template + "; skipping");
    return;
}

if fs::copy_if_missing(template, target) {
    log("wrote .env from " + template);
    return;
}

log("[info] preserving existing " + target);
