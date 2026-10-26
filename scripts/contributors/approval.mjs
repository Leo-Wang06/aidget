import { appendFile, readFile, writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

const API_ROOT = 'https://api.github.com';
export const APPROVED_FILE = '.github/APPROVED_CONTRIBUTORS';
const APPROVER_PERMISSIONS = new Set(['admin']);
export const UNAPPROVED_PR_MESSAGE = [
  'This pull request was closed because you are not approved to contribute code.',
  '',
  'Open a Contribution Proposal Issue and discuss the change with maintainers.',
  'Admins can reply `lgtmi` to approve for future pull requests.',
].join('\n');

export function parseApprovalCommand(body) {
  const firstLine = String(body ?? '')
    .trim()
    .split(/\r?\n/, 1)[0]
    .trim();
  return /^lgtmi(?:$|[\s.!?,:;])/iu.test(firstLine);
}

export function canApprove(permission) {
  return APPROVER_PERMISSIONS.has(permission);
}

export function parseApprovedContributors(content) {
  const contributors = new Set();
  for (const rawLine of content.split('\n')) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) {
      continue;
    }
    contributors.add(line.toLowerCase());
  }
  return contributors;
}

export function addApprovedContributor(content, username) {
  const contributors = parseApprovedContributors(content);
  if (contributors.has(username.toLowerCase())) {
    return { content, status: 'already' };
  }

  const separator = content.endsWith('\n') ? '' : '\n';
  return {
    content: `${content}${separator}${username}\n`,
    status: 'changed',
  };
}

/**
 * Builds the public issue comment for a contributor approval result.
 * @param {"changed" | "already"} status
 * @param {string} username
 * @returns {string | null} The comment, or null when an existing approval should stay silent.
 */
export function formatApprovalComment(status, username) {
  if (status === 'changed') {
    return `@${username}, you are approved to open pull requests.`;
  }
  if (status === 'already') {
    return null;
  }
  throw new Error(`unexpected approval status: ${String(status)}`);
}

async function request(token, method, path, body) {
  const response = await fetch(`${API_ROOT}${path}`, {
    method,
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'X-GitHub-Api-Version': '2022-11-28',
      ...(body ? { 'Content-Type': 'application/json' } : {}),
    },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });

  if (!response.ok) {
    const detail = (await response.text()).slice(0, 500);
    throw new Error(`${method} ${path} failed with ${response.status}: ${detail}`);
  }

  if (response.status === 204) {
    return null;
  }
  return response.json();
}

async function getPermission(token, repository, username) {
  const response = await fetch(
    `${API_ROOT}/repos/${repository}/collaborators/${encodeURIComponent(username)}/permission`,
    {
      headers: {
        Accept: 'application/vnd.github+json',
        Authorization: `Bearer ${token}`,
        'X-GitHub-Api-Version': '2022-11-28',
      },
    },
  );

  if (response.status === 404) {
    return 'none';
  }
  if (!response.ok) {
    throw new Error(`GET collaborator permission for ${username} failed with ${response.status}`);
  }

  return (await response.json()).permission;
}

async function setOutput(name, value) {
  if (!process.env.GITHUB_OUTPUT) {
    return;
  }
  await appendFile(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
}

async function updateApproval() {
  if (!parseApprovalCommand(process.env.COMMENT_BODY)) {
    await setOutput('status', 'skipped');
    return;
  }

  const repository = process.env.GITHUB_REPOSITORY;
  const commenter = process.env.COMMENTER;
  const issueAuthor = process.env.ISSUE_AUTHOR;
  const token = process.env.GITHUB_TOKEN;

  if (!repository || !commenter || !issueAuthor || !token) {
    throw new Error('missing GitHub issue context');
  }

  const permitted = canApprove(await getPermission(token, repository, commenter));
  if (!permitted) {
    console.log(`${commenter} cannot approve contributors`);
    await setOutput('status', 'skipped');
    return;
  }

  if (issueAuthor.endsWith('[bot]')) {
    console.log(`refusing to approve bot account ${issueAuthor}`);
    await setOutput('status', 'skipped');
    return;
  }

  const content = await readFile(APPROVED_FILE, 'utf8');
  const result = addApprovedContributor(content, issueAuthor);
  if (result.status === 'changed') {
    await writeFile(APPROVED_FILE, result.content);
  }

  await setOutput('status', result.status);
  await setOutput('username', issueAuthor);
}

async function commentApproval() {
  const status = process.env.APPROVAL_STATUS;
  const repository = process.env.GITHUB_REPOSITORY;
  const issueNumber = process.env.ISSUE_NUMBER;
  const issueAuthor = process.env.APPROVAL_USERNAME;
  const token = process.env.GITHUB_TOKEN;
  const body = formatApprovalComment(status, issueAuthor);
  if (body === null) {
    return;
  }

  if (!repository || !issueNumber || !issueAuthor || !token) {
    throw new Error('missing GitHub issue context');
  }

  await request(token, 'POST', `/repos/${repository}/issues/${issueNumber}/comments`, { body });
}

async function gatePullRequest() {
  const repository = process.env.GITHUB_REPOSITORY;
  const number = process.env.PR_NUMBER;
  const author = process.env.PR_AUTHOR;
  const association = process.env.PR_AUTHOR_ASSOCIATION;
  const token = process.env.GITHUB_TOKEN;
  if (!repository || !number || !author || !token) {
    throw new Error('missing pull request context');
  }

  if (['OWNER', 'MEMBER', 'COLLABORATOR'].includes(association)) {
    console.log(`${author} already has repository access`);
    return;
  }
  if (author.endsWith('[bot]')) {
    console.log(`allowing bot account ${author}`);
    return;
  }

  const approved = parseApprovedContributors(await readFile(APPROVED_FILE, 'utf8'));
  if (approved.has(author.toLowerCase())) {
    console.log(`${author} is approved to open pull requests`);
    return;
  }

  await request(token, 'POST', `/repos/${repository}/issues/${number}/comments`, {
    body: UNAPPROVED_PR_MESSAGE,
  });
  await request(token, 'PATCH', `/repos/${repository}/pulls/${number}`, {
    state: 'closed',
  });
}

async function main() {
  const command = process.argv[2];
  if (command === 'update') {
    await updateApproval();
  } else if (command === 'comment') {
    await commentApproval();
  } else if (command === 'gate') {
    await gatePullRequest();
  } else {
    throw new Error('usage: approval.mjs <update|comment|gate>');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(`contributor approval: ${error.message}`);
    process.exitCode = 1;
  });
}
