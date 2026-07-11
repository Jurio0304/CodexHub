import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

const cjk = /[\u3400-\u9fff]/u;
const allowedLiteralValues = new Set(["简体中文"]);
const violations = [];
let uiCopyDeclaration = null;

function sourceFiles(root) {
  return fs.readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const fullPath = path.join(root, entry.name);
    if (entry.isDirectory()) return sourceFiles(fullPath);
    return /\.tsx?$/u.test(entry.name) && !fullPath.includes(`${path.sep}generated${path.sep}`)
      ? [fullPath]
      : [];
  });
}

function insideCopyRegistry(node) {
  for (let current = node; current; current = current.parent) {
    if (ts.isVariableDeclaration(current) && ts.isIdentifier(current.name)) {
      return current.name.text === "uiCopy" || /Copy$/u.test(current.name.text);
    }
  }
  return false;
}

function literalText(node) {
  if (ts.isStringLiteralLike(node) || ts.isTemplateHead(node) || ts.isTemplateMiddle(node) || ts.isTemplateTail(node)) {
    return node.text;
  }
  return null;
}

for (const filePath of sourceFiles("src")) {
  const source = ts.createSourceFile(
    filePath,
    fs.readFileSync(filePath, "utf8"),
    ts.ScriptTarget.Latest,
    true,
    filePath.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS
  );
  const visit = (node) => {
    if (
      filePath.endsWith(`${path.sep}App.tsx`) &&
      ts.isVariableDeclaration(node) &&
      ts.isIdentifier(node.name) &&
      node.name.text === "uiCopy"
    ) {
      uiCopyDeclaration = node;
    }
    const value = literalText(node);
    if (value && cjk.test(value) && !allowedLiteralValues.has(value) && !insideCopyRegistry(node)) {
      const position = source.getLineAndCharacterOfPosition(node.getStart(source));
      violations.push(`${filePath}:${position.line + 1}:${position.character + 1} ${JSON.stringify(value)}`);
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
}

function unwrapExpression(node) {
  let current = node;
  while (
    current &&
    (ts.isAsExpression(current) ||
      ts.isSatisfiesExpression(current) ||
      ts.isParenthesizedExpression(current))
  ) {
    current = current.expression;
  }
  return current;
}

function propertyKey(property) {
  if (!property.name) return null;
  if (ts.isIdentifier(property.name) || ts.isStringLiteralLike(property.name)) return property.name.text;
  return null;
}

function propertyValue(object, key) {
  const property = object.properties.find((candidate) => propertyKey(candidate) === key);
  return property && ts.isPropertyAssignment(property) ? unwrapExpression(property.initializer) : null;
}

function collectShape(object, prefix = "") {
  const paths = new Set();
  for (const property of object.properties) {
    const key = propertyKey(property);
    if (!key) continue;
    const nextPath = prefix ? `${prefix}.${key}` : key;
    if (ts.isPropertyAssignment(property)) {
      const value = unwrapExpression(property.initializer);
      if (value && ts.isObjectLiteralExpression(value)) {
        for (const nested of collectShape(value, nextPath)) paths.add(nested);
        continue;
      }
    }
    paths.add(nextPath);
  }
  return paths;
}

if (!uiCopyDeclaration?.initializer) {
  throw new Error("Could not locate the uiCopy registry in src/App.tsx.");
}
const registry = unwrapExpression(uiCopyDeclaration.initializer);
if (!registry || !ts.isObjectLiteralExpression(registry)) {
  throw new Error("uiCopy must remain an object literal so bilingual keys can be verified.");
}
const en = propertyValue(registry, "en");
const zh = propertyValue(registry, "zh");
if (!en || !zh || !ts.isObjectLiteralExpression(en) || !ts.isObjectLiteralExpression(zh)) {
  throw new Error("uiCopy must contain object-literal en and zh registries.");
}
const enShape = collectShape(en);
const zhShape = collectShape(zh);
const missingInZh = [...enShape].filter((key) => !zhShape.has(key));
const missingInEn = [...zhShape].filter((key) => !enShape.has(key));
if (missingInZh.length > 0 || missingInEn.length > 0) {
  throw new Error(
    `Bilingual copy keys are incomplete:\nmissing in zh: ${missingInZh.join(", ") || "none"}\nmissing in en: ${missingInEn.join(", ") || "none"}`
  );
}

const appSource = fs.readFileSync("src/App.tsx", "utf8");
for (const token of [
  '"Refresh latest Codex version": "刷新 Codex 最新版本"',
  '"Import cc-switch profiles": "导入 cc-switch 配置"',
  'unknownAction: "后台任务"',
  'genericError: "操作失败，请在任务详情或日志中查看诊断信息。"',
  "function localizeTaskSummary(task: TaskRun, copy: UICopy)",
  "function localizeFeedbackMessage(message: string, copy: UICopy, tone: FeedbackTone)"
]) {
  if (!appSource.includes(token)) {
    throw new Error(`Task localization contract is incomplete: ${token}`);
  }
}
if (/<(?:td|strong)>\{task\.summary\}<\//u.test(appSource)) {
  throw new Error("Task summaries must render through localizeTaskSummary instead of raw backend English.");
}

if (violations.length > 0) {
  throw new Error(`Hard-coded Chinese UI strings must live in a copy registry:\n${violations.join("\n")}`);
}

console.log("I18N PASS: Chinese UI strings stay inside copy registries and en/zh keys match.");
