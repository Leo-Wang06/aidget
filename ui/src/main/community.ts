import { invoke } from '@tauri-apps/api/core';
import { createElement, LayoutGrid, ShieldCheck } from 'lucide';

import type {
  CommunityEntry,
  CommunitySnapshot,
  DownloadCount,
  PackageAuthor,
  WidgetPlatform,
} from '../types/rust';
import { translate, type Locale, type MessageKey } from './locale';
import { resolvedLocale } from './preferences';

/** Format npm's count without confusing an absent response with a real zero. */
export function weeklyDownloads(value: number | null, locale: string) {
  if (value === null) {
    return null;
  }
  const [divisor, suffix] =
    value >= 1e9 ? [1e9, 'b'] : value >= 1e6 ? [1e6, 'm'] : value >= 1e3 ? [1e3, 'k'] : [1, ''];
  return (
    new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value / divisor) + suffix
  );
}
function authorName(author: PackageAuthor | null) {
  return typeof author === 'string' ? author : (author?.name ?? '');
}
const platformNames: Record<WidgetPlatform, string> = {
  macos: 'macOS',
  windows: 'Windows',
  linux: 'Linux',
};

/** Render a community catalogue. Remote HTML never enters this document. */
export function createCommunityPage(
  root: HTMLElement,
  locale: () => Locale,
  installed: () => Promise<void>,
  hasGithubToken: () => boolean,
) {
  let snapshot: CommunitySnapshot | undefined;
  let loading = false;
  let requested = false;
  let generation = 0;
  let installing: string | undefined;
  let selected: { entry: CommunityEntry; commit: string } | undefined;
  let installError = '';
  const counts = new Map<string, DownloadCount>();
  const media = new Map<string, Promise<string>>();
  const t = (key: MessageKey) => translate(resolvedLocale(locale()), key);
  const header = document.createElement('div');
  header.className = 'community-heading';
  const heading = document.createElement('h1');
  heading.id = 'community-widgets-title';
  const refresh = document.createElement('button');
  refresh.type = 'button';
  refresh.className = 'community-refresh secondary-button';
  const status = document.createElement('p');
  status.className = 'community-status';
  status.setAttribute('role', 'status');
  const grid = document.createElement('div');
  grid.className = 'widget-grid community-grid';
  const dialog = document.createElement('dialog');
  dialog.className = 'community-dialog';
  dialog.tabIndex = -1;
  dialog.setAttribute('aria-labelledby', 'community-detail-title');
  header.append(heading, refresh);
  root.replaceChildren(header, status, grid, dialog);
  refresh.addEventListener('click', () => void load());
  dialog.addEventListener('close', () => {
    selected = undefined;
  });

  function mediaUrl(entry: CommunityEntry, path: string, commit = snapshot!.commit) {
    const key = `${commit}/${entry.entry.package.name}/${path}`;
    let pending = media.get(key);
    if (!pending) {
      pending = invoke<string>('community_media', {
        commit,
        package: entry.entry.package.name,
        path,
      });
      media.set(key, pending);
    }
    return pending;
  }
  function logo(entry: CommunityEntry) {
    const box = document.createElement('span');
    box.className = 'community-logo';
    box.setAttribute('aria-hidden', 'true');
    const placeholder = document.createElement('span');
    placeholder.className = 'community-logo-default';
    placeholder.append(createElement(LayoutGrid, { width: 24, height: 24, 'stroke-width': 1.6 }));
    box.append(placeholder);
    if (entry.entry.config.logo) {
      const image = document.createElement('img');
      image.alt = '';
      image.width = 32;
      image.height = 32;
      void mediaUrl(entry, entry.entry.config.logo)
        .then((url) => {
          image.src = url;
          box.replaceChildren(image);
        })
        .catch((error) => {
          box.title = `${t('communityLogoFailed')}: ${String(error)}`;
        });
    }
    return box;
  }
  function screenshot(entry: CommunityEntry, path: string, commit = snapshot!.commit) {
    const box = document.createElement('div');
    box.className = 'preview-stage community-preview';
    const image = document.createElement('img');
    image.alt = entry.entry.config.title;
    image.className = 'preview-image';
    image.width = 720;
    image.height = 440;
    image.decoding = 'async';
    const message = document.createElement('span');
    message.textContent = t('loading');
    box.append(message);
    void mediaUrl(entry, path, commit)
      .then((url) => {
        image.onload = () => box.replaceChildren(image);
        image.onerror = () => {
          message.textContent = t('communityScreenshotFailed');
        };
        image.src = url;
      })
      .catch((error) => {
        message.textContent = t('communityScreenshotFailed');
        message.title = String(error);
      });
    return box;
  }
  function actionLabel(entry: CommunityEntry) {
    if (installing === entry.entry.package.name) {
      return t('communityInstalling');
    }
    if (entry.installed) {
      return t('communityInstalled');
    }
    if (entry.idConflict) {
      return t('communityIdConflict');
    }
    if (!entry.supported) {
      return t('communityUnsupported');
    }
    return t('communityInstall');
  }
  function installAction(entry: CommunityEntry, confirm: boolean, commit = snapshot!.commit) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'community-install secondary-button';
    button.textContent = actionLabel(entry);
    button.disabled =
      loading || !!installing || entry.installed || entry.idConflict || !entry.supported;
    button.addEventListener('click', () => {
      if (confirm) {
        void install(entry, commit);
      } else {
        showDetails(entry);
      }
    });
    return button;
  }
  function permissions(entry: CommunityEntry) {
    const list = document.createElement('ul');
    list.className = 'community-badges community-permissions';
    const names: Record<string, MessageKey> = {
      script: 'permissionScript',
      location: 'permissionLocation',
      storage: 'permissionStorage',
    };
    for (const permission of entry.entry.config.permissions) {
      const item = document.createElement('li');
      item.textContent = t(names[permission]);
      list.append(item);
    }
    if (!list.children.length) {
      const item = document.createElement('li');
      item.textContent = t('communityNoPermissions');
      list.append(item);
    }
    return list;
  }
  function renderCard(entry: CommunityEntry) {
    const { config, package: pkg } = entry.entry;
    const card = document.createElement('article');
    card.className = 'widget-card community-card';
    card.dataset.package = pkg.name;
    const top = document.createElement('div');
    top.className = 'widget-info community-card-heading';
    const identity = document.createElement('div');
    identity.className = 'community-identity';
    const name = document.createElement('button');
    name.type = 'button';
    name.className = 'widget-title community-name';
    name.textContent = config.title;
    name.disabled = loading || !!installing;
    name.title = config.title;
    name.addEventListener('click', () => showDetails(entry));
    const by = document.createElement('span');
    by.className = 'community-verified';
    const verifiedLabel = document.createElement('span');
    verifiedLabel.textContent = t('communityVerified');
    by.append(
      createElement(ShieldCheck, {
        width: 13,
        height: 13,
        'stroke-width': 1.7,
        'aria-hidden': 'true',
      }),
      verifiedLabel,
    );
    by.title = authorName(pkg.author);
    identity.append(name, by);
    top.append(logo(entry), identity, installAction(entry, false));
    const preview = document.createElement('button');
    preview.type = 'button';
    preview.disabled = loading || !!installing;
    preview.className = 'community-preview-button';
    preview.setAttribute('aria-label', `${t('communityDetails')}: ${config.title}`);
    preview.addEventListener('click', () => showDetails(entry));
    preview.append(screenshot(entry, config.screenshots[0]));
    const footer = document.createElement('div');
    footer.className = 'community-card-footer';
    const count = document.createElement('span');
    count.dataset.downloadCount = pkg.name;
    renderCount(count, pkg.name);
    const version = document.createElement('span');
    version.textContent = `v${pkg.version}`;
    footer.append(count, version);
    card.append(top, preview, footer);
    return card;
  }
  function renderCount(element: HTMLElement, name: string) {
    const count = counts.get(name);
    const number = weeklyDownloads(count?.weekly ?? null, resolvedLocale(locale()));
    element.hidden = number === null;
    element.textContent = number === null ? '' : `↓ ${number}${t('communityPerWeek')}`;
    element.title = `${t('communityNpmDownloads')}${count?.weekly === null ? '' : `: ${count?.weekly ?? ''}`}`;
  }
  function render() {
    heading.textContent = t('communityWidgetsTitle');
    refresh.textContent = t(loading ? 'communityRefreshing' : 'communityRefresh');
    refresh.disabled = loading || !!installing;
    root.setAttribute('aria-busy', String(loading));
    if (loading) {
      status.textContent = '';
    } else if (snapshot) {
      status.textContent =
        installError && !dialog.open
          ? installError
          : snapshot.widgets.length
            ? ''
            : t('communityEmpty');
    }
    grid.replaceChildren(...(snapshot?.widgets.map(renderCard) ?? []));
    if (selected && dialog.open) {
      renderDetails(selected.entry, selected.commit);
    }
  }
  function renderDetails(entry: CommunityEntry, commit: string) {
    const { config, package: pkg } = entry.entry;
    const title = document.createElement('h2');
    title.id = 'community-detail-title';
    title.textContent = config.title;
    const description = document.createElement('p');
    description.className = 'community-detail-description';
    description.textContent = pkg.description;
    const identity = document.createElement('p');
    identity.className = 'community-detail-identity';
    identity.textContent = `${authorName(pkg.author)} · ${pkg.name} · v${pkg.version}`;
    const permissionsTitle = document.createElement('h3');
    permissionsTitle.textContent = t('communityPermissions');
    const platformsTitle = document.createElement('h3');
    platformsTitle.textContent = t('communityPlatforms');
    const platforms = document.createElement('ul');
    platforms.className = 'community-badges community-platforms';
    for (const platform of config.platforms) {
      const item = document.createElement('li');
      item.textContent = platformNames[platform];
      platforms.append(item);
    }
    const networkTitle = document.createElement('h3');
    networkTitle.textContent = t('communityNetwork');
    let network: HTMLElement;
    if (config.network.length) {
      const list = document.createElement('ul');
      list.className = 'community-network';
      for (const origin of config.network) {
        const item = document.createElement('li');
        item.textContent = origin;
        list.append(item);
      }
      network = list;
    } else {
      network = document.createElement('p');
      network.className = 'community-network-empty';
      network.textContent = t('communityNoNetwork');
    }
    const images = document.createElement('div');
    images.className = 'community-detail-images';
    images.append(...config.screenshots.map((path) => screenshot(entry, path, commit)));
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'community-dialog-close';
    close.textContent = t('communityClose');
    close.addEventListener('click', () => dialog.close());
    const error = document.createElement('p');
    error.textContent = installError;
    error.className = 'community-install-error';
    error.setAttribute('role', 'alert');
    const header = document.createElement('div');
    header.className = 'community-dialog-header';
    header.append(title, installAction(entry, true, commit));
    const content = document.createElement('div');
    content.className = 'community-dialog-content';
    content.append(
      identity,
      images,
      description,
      platformsTitle,
      platforms,
      permissionsTitle,
      permissions(entry),
      networkTitle,
      network,
    );
    const footer = document.createElement('div');
    footer.className = 'community-dialog-footer';
    footer.append(error, close);
    dialog.replaceChildren(header, content, footer);
  }
  function showDetails(entry: CommunityEntry) {
    installError = '';
    selected = { entry, commit: snapshot!.commit };
    renderDetails(entry, selected.commit);
    dialog.showModal();
    dialog.focus();
  }
  async function install(entry: CommunityEntry, commit: string) {
    if (!snapshot || installing) {
      return;
    }
    installing = entry.entry.package.name;
    installError = '';
    render();
    try {
      await invoke('community_install', { commit, package: entry.entry.package.name });
      entry.installed = true;
      await installed();
      dialog.close();
    } catch (error) {
      installError = `${t('communityInstallFailed')}: ${String(error)}`;
    } finally {
      installing = undefined;
      render();
    }
  }
  async function load() {
    if (loading || installing) {
      return;
    }
    const current = ++generation;
    requested = true;
    loading = true;
    counts.clear();
    media.clear();
    render();
    try {
      const next = await invoke<CommunitySnapshot>('community_list');
      if (current !== generation) {
        return;
      }
      snapshot = next;
      void invoke<DownloadCount[]>('community_downloads', {
        packages: next.widgets.map((e) => e.entry.package.name),
      })
        .then((values) => {
          if (current !== generation) {
            return;
          }
          for (const value of values) {
            counts.set(value.package, value);
          }
          grid
            .querySelectorAll<HTMLElement>('[data-download-count]')
            .forEach((element) => renderCount(element, element.dataset.downloadCount!));
        })
        .catch((error) => {
          status.title = `${t('communityDownloadsUnavailable')}: ${String(error)}`;
        });
    } catch (error) {
      snapshot = undefined;
      status.textContent = `${t('communityLoadFailed')}: ${String(error)}${
        hasGithubToken() ? '' : `\n${t('communityGithubTokenHint')}`
      }`;
    } finally {
      loading = false;
      render();
    }
  }
  return {
    /** Fetch on first visit; explicit refresh retrieves a new main snapshot. */
    show() {
      if (!requested) {
        void load();
      } else {
        render();
      }
    },
    /** Local changes invalidate installed state and are reconciled on next visit. */
    invalidate() {
      requested = false;
    },
    /** Update translated labels without an additional network request. */
    render,
  };
}
