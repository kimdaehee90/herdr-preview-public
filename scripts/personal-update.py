#!/usr/bin/env python3
"""Back up a personal Herdr build and merge upstream in an isolated worktree.

Never fetches, installs, restarts an app/core, resets a branch, or rewrites a profile.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tarfile
import uuid

ROOT = Path(__file__).resolve().parent.parent


def run(root, *args, check=True):
    result = subprocess.run(args, cwd=root, text=True, capture_output=True)
    if check and result.returncode:
        raise RuntimeError(f'{args[0]} failed: {result.stderr or result.stdout}')
    return result


def git(root, *args):
    return run(root, 'git', *args).stdout.strip()


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def archive(destination, paths, required=False):
    paths = list(paths)
    missing = [str(source) for source, _ in paths if not source.exists()]
    if required and missing:
        raise RuntimeError('Required backup resource missing: ' + ', '.join(missing))
    if missing and len(missing) == len(paths):
        return {'status': 'missing', 'required': required, 'archive': None, 'files': 0}
    count = 0
    def regular(member):
        nonlocal count
        if member.isfile() or member.issym() or member.islnk(): count += 1
        return member if member.isfile() or member.isdir() or member.issym() or member.islnk() else None
    with tarfile.open(destination, 'w:gz', compresslevel=1) as target:
        for source, name in paths:
            if source.exists():
                target.add(source, arcname=name, filter=regular)
    if required and count == 0:
        raise RuntimeError('Required backup resource is empty: ' + str(destination))
    return {'status': 'archived', 'required': required, 'archive': destination.name, 'files': count}


def backup(root, destination, config, app, profile=None, core=None, core_config=None, core_state=None, installed=False):
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:6]
    folder = destination / stamp
    folder.mkdir(parents=True, mode=0o700)
    folder.chmod(0o700)
    run(root, 'git', 'bundle', 'create', str(folder / 'source.bundle'), '--all')
    # HEAD -> working tree includes staged and unstaged tracked changes.
    (folder / 'working-tree.patch').write_text(run(root, 'git', 'diff', '--binary', 'HEAD').stdout)
    (folder / 'source-status.txt').write_text(git(root, 'status', '--short') + '\n')
    names = run(root, 'git', 'ls-files', '--others', '--exclude-standard', '-z').stdout.split('\0')
    archive(folder / 'untracked-source.tar.gz', [(root / p, p) for p in names if p])
    resources = {
        'settings': archive(folder / 'personal-settings.tar.gz', [(config, 'herdr-preview')], required=installed),
        'app': archive(folder / 'personal-app.tar.gz', [(app, 'Herdr Preview.app')], required=installed),
    }
    if profile:
        resources['webkit'] = archive(folder / 'webkit-profile-live-copy.tar.gz', [(profile, 'dev.herdr.preview')])
    if core:
        if not core.is_file(): raise RuntimeError('Required core binary missing: ' + str(core))
        shutil.copy2(core, folder / 'herdr-core')
        resources['core_binary'] = {'status': 'copied', 'required': True, 'archive': 'herdr-core', 'files': 1}
    if core_config:
        resources['core_config'] = archive(folder / 'core-config.tar.gz', [(core_config, 'herdr-core')])
    if core_state:
        resources['core_state'] = archive(folder / 'core-state-live-copy.tar.gz', [(core_state, 'herdr-core-state')], required=bool(core))
    manifest = {
        'schema': 2, 'created_utc': stamp, 'source_head': git(root, 'rev-parse', 'HEAD'),
        'source_branch': git(root, 'branch', '--show-current'),
        'config_home': str(config), 'bundle_id': 'dev.herdr.preview',
        'profile_path': str(profile) if profile else None,
        'profile_copy_is_live': bool(profile),
        'core_state_path': str(core_state) if core_state else None,
        'resources': resources,
        'note': 'Live WebKit/core-state copies are best-effort, not transactional snapshots. Core session/layout JSON and screen files are included; sockets/special files are excluded. Live PTY processes cannot be restored from an archive. Never restore over a running app/core/profile.',
        'files': {p.name: digest(p) for p in folder.iterdir() if p.is_file()},
    }
    (folder / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')
    return folder


def verify_backup(folder):
    manifest = json.loads((folder / 'manifest.json').read_text())
    for name, sha in manifest['files'].items():
        if Path(name).name != name or digest(folder / name) != sha:
            raise RuntimeError('Backup checksum mismatch: ' + name)
    for resource in manifest.get('resources', {}).values():
        if resource['required'] and (resource['status'] == 'missing' or not resource['files']):
            raise RuntimeError('Required backup resource absent or empty')
        if resource['archive'] and resource['archive'] not in manifest['files']:
            raise RuntimeError('Backup resource not covered by checksum')
    return manifest


def assert_clean(root):
    if git(root, 'status', '--porcelain', '--untracked-files=normal'):
        raise RuntimeError('Commit personal source changes before preparing an update; the live checkout will not be modified.')


def prepare(root, upstream, destination):
    assert_clean(root)
    upstream_sha = git(root, 'rev-parse', '--verify', upstream + '^{commit}')
    current = git(root, 'rev-parse', 'HEAD')
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:6]
    branch = 'herdr-preview-update-' + stamp
    # App control sockets have a 103-byte macOS path limit. Keep QA homes short.
    worktree = destination.parent / ('hu-' + uuid.uuid4().hex[:8])
    if len(os.fsencode(worktree / '.browser-test/app/herdr-app.sock')) > 103:
        raise RuntimeError('Use a shorter candidate directory for native UI validation')
    destination.mkdir(parents=True, exist_ok=True, mode=0o700)
    run(root, 'git', 'worktree', 'add', '-b', branch, str(worktree), current)
    merge = run(worktree, 'git', 'merge', '--no-ff', '--no-edit', upstream_sha, check=False)
    if merge.returncode:
        conflicts = git(worktree, 'diff', '--name-only', '--diff-filter=U').splitlines()
        state = 'conflicts' if conflicts else 'merge_failed'
    else:
        conflicts, state = [], 'merged_needs_validation'
    result = {
        'schema': 1, 'state': state, 'previous_head': current, 'upstream': upstream_sha,
        'worktree': str(worktree), 'branch': branch, 'conflicts': conflicts,
        'merge_output': merge.stdout + merge.stderr, 'installed': False,
    }
    report = destination / ('candidate-' + stamp + '.json')
    report.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    result['report'] = str(report)
    return result


def verify_sources(root):
    config = json.loads((root / 'personal-build.json').read_text())
    for name in config['required_sources']:
        path = root / name
        # Reject deleted/empty/stubbed feature files before a build can be certified.
        if not path.is_file() or len(path.read_text().strip()) < 100:
            raise RuntimeError('Personal feature source missing or incomplete: ' + name)
    return config


def verify_candidate(root, binary):
    assert_clean(root)
    config = verify_sources(root)
    info = json.loads(run(root, str(binary), '--personal-build-info').stdout)
    if info.get('protected') is not True or info.get('bundle_id') != config['bundle_id'] or info.get('config_home') != config['config_home']:
        raise RuntimeError('Candidate is missing personal update/identity protection')
    commands = json.loads(run(root, str(binary), 'commands').stdout)
    missing = set(config['required_commands']) - {c['name'] for c in commands}
    if missing:
        raise RuntimeError('Personal commands missing: ' + ', '.join(sorted(missing)))
    return {'head': git(root, 'rev-parse', 'HEAD'), 'binary_sha256': digest(binary), 'build': info}


def validate(root, tools_root):
    """A merge alone is not an update: build/test without touching the installed app."""
    assert_clean(root)
    env = os.environ.copy()
    env['PATH'] = os.pathsep.join([str(Path.home()/'.cargo/bin'), str(tools_root/'.build-tools/bun-darwin-aarch64'), '/opt/homebrew/bin', env.get('PATH', '')])
    env['ZIG'] = str(tools_root/'.build-tools/zig-aarch64-macos-0.16.0/zig')
    env['CARGO_PROFILE_DEV_DEBUG'] = '1'
    env.pop('HERDR_APP_RELEASE', None)
    env.pop('HERDR_RELEASES_URL', None)
    for key in ('HERDR_APP_HOME', 'HERDR_APP_ID', 'HC_SESSION', 'HC_SOCKET', 'HERDR_PREVIEW_TEST_CONTROL'):
        env.pop(key, None)
    # Unit tests also get an isolated home, before native scripts set their own.
    env['HERDR_APP_HOME'] = str(root/'.personal-update/unit-home')
    env['HERDR_APP_ID'] = 'herdr-validation'
    env['HC_SESSION'] = 'unitqa'
    steps = [
        ['bun', 'test', 'scripts'],
        ['python3', 'scripts/test-personal-update.py'],
        ['cargo', 'test', '--locked', '--features', 'personal-preview', '--config', 'profile.dev.package."*".opt-level=0'],
        ['cargo', 'build', '--locked', '--features', 'personal-preview', '--config', 'profile.dev.package."*".opt-level=0'],
        ['cargo', 'build', '--locked', '--release', '--features', 'personal-preview'],
        ['bun', 'scripts/docs.ts', 'check', str(root/'target/release/herdr-app')],
    ]
    logs = root / '.personal-update'
    logs.mkdir(exist_ok=True)
    for i, command in enumerate(steps):
        with (logs / f'check-{i}.log').open('w') as output:
            subprocess.run(command, cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT, check=True)
    session = 'up' + uuid.uuid4().hex[:6]
    home = root / '.browser-test/app'
    if (home / 'herdr-app.sock').exists():
        with socket.socket(socket.AF_UNIX) as client:
            try:
                client.connect(str(home/'herdr-app.sock'))
            except OSError:
                pass
            else:
                raise RuntimeError('QA home is already in use; refusing to touch its app/core')
    native = [
        ['python3','scripts/browser-smoke.py','--home',str(home),'--binary',str(root/'target/debug/herdr-app'),'--session',session],
        ['python3','scripts/preview-smoke.py','--home',str(home)],
        ['python3','scripts/browser-focus-smoke.py','--session',session],
    ]
    try:
        for i, command in enumerate(native):
            with (logs / f'native-{i}.log').open('w') as output:
                subprocess.run(command,cwd=root,env=env,stdout=output,stderr=subprocess.STDOUT,check=True)
    finally:
        # Only the uniquely named validation core may be stopped.
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(3)
            try:
                client.connect(str(home/'herdr-app.sock'))
                client.sendall(b'{"do":"app.quit"}\n')
                client.recv(1024)
            except OSError:
                pass
        subprocess.run([str(Path.home()/'.local/bin/hc'),'server','stop'],env={**env,'HC_SESSION':session},capture_output=True)
    result = verify_candidate(root, root/'target/release/herdr-app')
    result.update({'checks':steps+native,'installed':False,'native_ui_checks_passed':True})
    (logs/'validated.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('backup')
    p = commands.add_parser('verify-backup'); p.add_argument('folder', type=Path)
    p = commands.add_parser('prepare'); p.add_argument('upstream')
    p = commands.add_parser('validate'); p.add_argument('--tools-root', type=Path, default=ROOT)
    args = parser.parse_args()
    destination = ROOT.parent/'herdr-preview-backups'
    def save():
        user = Path.home()
        return backup(ROOT,destination,user/'.config/herdr-preview',user/'Applications/Herdr Preview.app',
                      user/'Library/WebKit/dev.herdr.preview',user/'.local/bin/herdr-core',user/'.config/herdr-core',
                      user/'.local/state/herdr-core',installed=(user/'.local/bin/herdr-preview').exists())
    if args.command == 'backup':
        folder = save()
        print(json.dumps({'backup':str(folder),'verified':bool(verify_backup(folder))}))
    elif args.command == 'verify-backup':
        print(json.dumps(verify_backup(args.folder), indent=2))
    elif args.command == 'prepare':
        assert_clean(ROOT)
        git(ROOT,'rev-parse','--verify',args.upstream+'^{commit}')
        saved = save()
        verify_backup(saved)
        result = prepare(ROOT,args.upstream,destination)
        result['backup'] = str(saved)
        Path(result['report']).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
        print(json.dumps(result,ensure_ascii=False,indent=2))
        if result['state'] != 'merged_needs_validation':
            raise SystemExit(2)
    else:
        print(json.dumps(validate(ROOT,args.tools_root),indent=2))


if __name__ == '__main__':
    main()
