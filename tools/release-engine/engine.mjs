// Reuse semantic-release's maintained parser, analyzer and notes generator.
// Fleab's Python adapter supplies original PR commits and tagged snapshots.
import { analyzeCommits } from '@semantic-release/commit-analyzer';
import { generateNotes } from '@semantic-release/release-notes-generator';
import semver from 'semver';
import { readFile } from 'node:fs/promises';

const logger = { log() {}, error() {}, warn() {} };
const parserOpts = {
  noteKeywords: ['BREAKING CHANGE', 'BREAKING-CHANGE'],
  // Keep every PR subject in the requested notes, including reverted commits.
  revertPattern: /(?!)/,
};
const config = {
  preset: 'conventionalcommits',
  parserOpts,
  releaseRules: [
    { breaking: true, release: 'major' },
    { type: 'feat', release: 'minor' },
    { type: '*', release: 'patch' },
  ],
};

function plain(value) {
  return String(value).replace(/\s+/g, ' ').trim()
    .replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')
    .replaceAll('@', '@\u200b').replace(/([\\`*_{}\[\]()#+!|])/g, '\\$1');
}

export async function bumpFor(commits) {
  if (!commits.length) throw Error('A merged PR must have at least one commit');
  let highest = 'patch';
  const rank = { patch: 0, minor: 1, major: 2 };
  // Analyze individually so a feat plus its revert still follows the user's
  // explicit "any feat" rule, instead of canceling out as a revert pair.
  for (const commit of commits) {
    const kind = await analyzeCommits(config, { commits: [commit], cwd: import.meta.dirname, logger }) || 'patch';
    if (rank[kind] > rank[highest]) highest = kind;
  }
  return highest;
}

export function nextVersion(previous, bump) {
  if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(previous)) throw Error('Invalid stable version');
  if (!['major', 'minor', 'patch'].includes(bump)) throw Error('Invalid release bump');
  return `v${semver.inc(previous.slice(1), bump)}`;
}

export async function notesFor({ tag, previous, bump, pr, commits }) {
  const url = new URL(pr.html_url);
  if (!['https:', 'http:'].includes(url.protocol) || /[\r\n()<>]/.test(pr.html_url)) throw Error('Invalid PR URL');
  for (const commit of commits) {
    if (!/^[0-9a-f]{40,64}$/.test(commit.hash)) throw Error('Invalid commit SHA');
  }
  const mainTemplate = await readFile(new URL('./notes.hbs', import.meta.url), 'utf8');
  return generateNotes({
    preset: 'conventionalcommits', parserOpts,
    writerOpts: {
      groupBy: 'type', commitGroupsSort: 'title', commitsSort: ['scope', 'subject'],
      mainTemplate,
      transform(commit) {
        return {
          ...commit, type: commit.type?.toLowerCase() || 'other', scope: commit.scope || 'general',
          subject: plain(commit.subject || commit.header || 'Untitled commit'),
          shortHash: commit.hash.slice(0, 7), breaking: Boolean(commit.notes?.length),
        };
      },
      finalizeContext(context) {
        return {
          ...context, bump,
          pr: { number: Number(pr.number), url: pr.html_url, title: plain(pr.title) },
          commitGroups: context.commitGroups.map(group => {
            const scopes = new Map();
            for (const commit of group.commits) {
              if (!scopes.has(commit.scope)) scopes.set(commit.scope, []);
              scopes.get(commit.scope).push(commit);
            }
            return {
              title: plain(group.title),
              scopes: [...scopes].sort(([a], [b]) => a.localeCompare(b)).map(([scope, commits]) => ({ scope: plain(scope), commits })),
            };
          }),
        };
      },
    },
  }, {
    cwd: import.meta.dirname, logger, commits,
    options: { repositoryUrl: pr.html_url.split('/pull/')[0] },
    lastRelease: { gitTag: previous, version: previous.slice(1) },
    nextRelease: { gitTag: tag, version: tag.slice(1) },
  });
}

let inputText = '';
for await (const chunk of process.stdin) inputText += chunk;
const input = JSON.parse(inputText);
let result;
if (input.action === 'analyze') result = { bump: await bumpFor(input.commits) };
else if (input.action === 'increment') result = { tag: nextVersion(input.previous, input.bump) };
else if (input.action === 'notes') result = { notes: await notesFor(input) };
else {
  const bump = await bumpFor(input.commits);
  const tag = nextVersion(input.previous, bump);
  result = { bump, tag, notes: await notesFor({ ...input, tag, bump }) };
}
process.stdout.write(JSON.stringify(result));
