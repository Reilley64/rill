#!/usr/bin/env python3
"""One-command, throwaway selected-source OpenSSL reference runner."""
from pathlib import Path
import datetime, hashlib, json, os, platform, shutil, subprocess, sys

ROOT = Path(__file__).resolve().parent
SOURCE = Path('/tmp/0045-source/openssl-45e844fa2a14ec92d146bd8f5778ac130b6625fb')
ARCHIVE = Path('/tmp/0045-source/source.tar.gz')
COMMIT = '45e844fa2a14ec92d146bd8f5778ac130b6625fb'
ARCHIVE_SHA256 = 'f54250b21795bb01240f3f9b42c384868d04460e339e39275aaaf93487f2d903'
WORK = Path('/tmp/0045-progress-reference-private')
SRC = WORK / ('openssl-' + COMMIT)
CERT = WORK / 'localhost-cert.pem'
KEY = WORK / 'localhost-key.pem'
EXE = WORK / 'reference'
JOBS = 4
COMMANDS = []

def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''): h.update(block)
    return h.hexdigest()

def run(args, cwd=None, capture=True):
    args = [str(x) for x in args]
    COMMANDS.append({'cwd': str(cwd or Path.cwd()), 'argv': args})
    print('+', ' '.join(args), file=sys.stderr, flush=True)
    return subprocess.run(args, cwd=cwd, text=True, stdout=subprocess.PIPE if capture else None,
                          stderr=subprocess.STDOUT if capture else None, check=True).stdout if capture else ''

def main():
    if not SOURCE.is_dir() or not ARCHIVE.is_file():
        raise SystemExit('required selected source/archive is absent')
    got = sha(ARCHIVE)
    if got != ARCHIVE_SHA256:
        raise SystemExit('archive SHA-256 mismatch: ' + got)
    version = (SOURCE / 'VERSION.dat').read_text()
    if not all(x in version for x in ('MAJOR=3', 'MINOR=5', 'PATCH=9')):
        raise SystemExit('selected source VERSION.dat is not 3.5.9')
    shutil.rmtree(WORK, ignore_errors=True)
    WORK.mkdir(parents=True)
    # Never configure or build the shared extracted source.
    shutil.copytree(SOURCE, SRC, symlinks=True)
    configure = ['./Configure', 'darwin64-arm64-cc', 'no-shared', 'no-tests', 'no-asm',
                 'no-dso', 'no-engine', 'no-async', '-O2']
    config_out = run(configure, SRC)
    build_out = run(['make', '-j%d' % JOBS, 'build_sw'], SRC)
    openssl = SRC / 'apps' / 'openssl'
    version_out = run([openssl, 'version', '-a'])
    run([openssl, 'req', '-config', SRC / 'apps' / 'openssl.cnf', '-x509', '-newkey', 'ec',
         '-pkeyopt', 'ec_paramgen_curve:P-256', '-nodes', '-days', '1',
         '-subj', '/CN=localhost', '-addext',
         'subjectAltName=DNS:localhost', '-keyout', KEY, '-out', CERT])
    cc = os.environ.get('CC', 'cc')
    compile_cmd = [cc, '-std=c11', '-O2', '-Wall', '-Wextra', '-Wpedantic',
                   '-I' + str(SRC / 'include'), ROOT / 'reference.c',
                   SRC / 'libssl.a', SRC / 'libcrypto.a', '-o', EXE]
    compile_out = run(compile_cmd)
    test = subprocess.run([str(EXE), str(CERT), str(KEY)], text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    COMMANDS.append({'cwd': str(Path.cwd()), 'argv': [str(EXE), '<ephemeral-cert>', '<ephemeral-key>']})
    print(test.stdout, end='')
    manifest = {
        'generated_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'claimed_selected_source_commit': COMMIT,
        'source_version': '3.5.9',
        'source_archive': str(ARCHIVE),
        'source_archive_sha256': got,
        'identity_limit': 'Archive hash and extracted directory name identify the supplied artifact; the tar extraction has no .git metadata, so commit identity was not independently recomputed.',
        'private_source_copy': str(SRC),
        'build_parallelism': JOBS,
        'configure_argv': configure,
        'platform': platform.platform(),
        'machine': platform.machine(),
        'python': platform.python_version(),
        'compiler': run([cc, '--version']).splitlines()[:2],
        'openssl_version_a': version_out.splitlines(),
        'commands': COMMANDS,
        'artifacts': {},
        'exit_code': test.returncode,
    }
    results = '# Actual execution results\n\n```text\n' + test.stdout + '```\n\n'
    results += 'Exit code: `%d`. See `README.md` for scope, findings, modeled partitions, and gaps.\n\n' % test.returncode
    results += ('## Renegotiation finding\n\n'
                'The earlier source-derived prediction of immediate `BAD_WRITE_RETRY` was falsified: '
                'the fresh write succeeds locally. The strengthened fixture preserves the prior ciphertext '
                'and then transfers every ciphertext byte made available after that write. The peer consumes '
                'the prior application record but fails record decryption before receiving the fresh byte; '
                'the warning-alert callback had fired, yet no warning-alert ciphertext was handed off and '
                'the sender record sequence had advanced. The explicit-stop fixture avoids issuing that write. '
                'This finite in-memory result supports the selected stop policy for the detected TLS 1.2 '
                'renegotiation path; it is not a complete provider/runtime/security proof.\n')
    (ROOT / 'results.md').write_text(results)
    for name in ('reference.c', 'run_reference.py', 'README.md', 'results.md'):
        path = ROOT / name
        if path.exists(): manifest['artifacts'][name] = {'sha256': sha(path), 'bytes': path.stat().st_size}
    (ROOT / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return test.returncode

if __name__ == '__main__':
    try:
        sys.exit(main())
    except subprocess.CalledProcessError as e:
        print(e.stdout or '', file=sys.stderr)
        print('COMMAND FAILED:', e.cmd, 'exit', e.returncode, file=sys.stderr)
        sys.exit(e.returncode)
