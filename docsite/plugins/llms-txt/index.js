// Build-time AI-discovery artifacts (CORE-507).
//
// After `docusaurus build`, writes into the output directory:
//   /llms.txt        summary + one link per published page
//   /llms-full.txt   every page's Markdown, concatenated
//   <permalink>.md   a clean Markdown copy of each page
//
// Content comes from plugin-content-docs, so exactly the published pages
// (no drafts, no unlisted pages) are covered. The Markdown has the YAML
// frontmatter removed and the page title prepended when the body has no H1.
const fs = require('fs');
const path = require('path');

const INSTALL_PATH = '/guides/getting-started';

function stripFrontmatter(text) {
  const m = text.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/);
  return (m ? text.slice(m[0].length) : text).replace(/^\s+/, '');
}

function mdPathFor(permalink) {
  const clean = permalink.replace(/\/+$/, '');
  return clean === '' ? '/index.md' : `${clean}.md`;
}

function pageMarkdown(doc, siteDir) {
  const file = doc.source.replace(/^@site\//, '');
  const abs = path.resolve(siteDir, file);
  let body = stripFrontmatter(fs.readFileSync(abs, 'utf8')).trimEnd();
  if (!/^#\s/m.test(body.split('\n').slice(0, 5).join('\n'))) {
    body = `# ${doc.title}\n\n${body}`;
  }
  return `${body}\n`;
}

// Only a description set in the page's frontmatter is used. The docs plugin
// also derives one from the first body line, which can stop mid-sentence.
function frontmatterDescription(doc, siteDir) {
  const file = doc.source.replace(/^@site\//, '');
  const text = fs.readFileSync(path.resolve(siteDir, file), 'utf8');
  const m = text.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (!m) return '';
  const d = m[1].match(/^description:\s*(.+)$/m);
  if (!d) return '';
  const v = d[1].trim().replace(/^(["'])(.*)\1$/, '$2').replace(/\s+/g, ' ');
  return /[.!?]$/.test(v) ? v : '';
}

function buildIndex(siteConfig, docs, siteDir) {
  const base = siteConfig.url.replace(/\/+$/, '');
  const abs = (p) => `${base}${p}`;
  const lines = [
    '# TypeAnvil',
    '',
    '> TypeAnvil is an open-source HTML and Markdown to PDF typesetting engine written in Rust. ' +
      'It lays out HTML or Markdown with paged-media CSS (@page, fragmentation, running headers, ' +
      'footnotes) and writes a PDF. These docs cover the self-hosted runtime: install, ' +
      'CLI, CSS support, specifications and operations.',
    '',
    'Use these docs when you need to install the `typeanvil` binary, render HTML or Markdown to PDF ' +
      'from the command line, or check which CSS features the engine supports.',
    '',
    'These docs are not a fit for the hosted cloud service, or for any task that needs ' +
      'JavaScript execution or interactive layout. The engine is a static renderer.',
    '',
    `Start with the install guide: ${abs(INSTALL_PATH + '.md')}`,
    '',
    `Full text of every page in one file: ${abs('/llms-full.txt')}`,
    '',
    '## Pages',
    '',
  ];
  const groups = new Map();
  for (const d of docs) {
    const seg = d.permalink.split('/').filter(Boolean)[0] || 'home';
    if (!groups.has(seg)) groups.set(seg, []);
    groups.get(seg).push(d);
  }
  for (const [seg, list] of groups) {
    lines.push(`### ${seg}`, '');
    for (const d of list) {
      const fm = frontmatterDescription(d, siteDir);
      const desc = fm ? `: ${fm}` : '';
      lines.push(`- [${d.title}](${abs(mdPathFor(d.permalink))})${desc}`);
    }
    lines.push('');
  }
  return lines.join('\n');
}

module.exports = function llmsTxtPlugin(context) {
  return {
    name: 'typeanvil-llms-txt',
    async postBuild({outDir, siteConfig, plugins}) {
      const versions = plugins
        .filter((p) => p.name === 'docusaurus-plugin-content-docs')
        .flatMap((p) => (p.content && p.content.loadedVersions) || []);
      const docs = versions.flatMap((v) => v.docs);
      if (docs.length === 0) {
        throw new Error('llms-txt: no published docs found');
      }
      docs.sort((a, b) => a.permalink.localeCompare(b.permalink));

      const full = [];
      for (const d of docs) {
        const md = pageMarkdown(d, context.siteDir);
        const dest = path.join(outDir, mdPathFor(d.permalink));
        fs.mkdirSync(path.dirname(dest), {recursive: true});
        fs.writeFileSync(dest, md);
        full.push(`<!-- Source: ${siteConfig.url.replace(/\/+$/, '')}${d.permalink} -->\n\n${md}`);
      }
      fs.writeFileSync(path.join(outDir, 'llms.txt'), buildIndex(siteConfig, docs, context.siteDir) + '\n');
      fs.writeFileSync(path.join(outDir, 'llms-full.txt'), full.join('\n---\n\n'));
    },
  };
};
