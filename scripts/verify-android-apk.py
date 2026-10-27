#!/usr/bin/env python3
"""Check the actual install artifact, including the executable engine payload."""
import os
import json
from pathlib import Path
import re
import struct
import subprocess
import sys
import zipfile


def verify(apk: Path) -> None:
    sdk = Path(os.environ['ANDROID_HOME'])
    build_tools = sdk / 'build-tools' / '36.0.0'
    subprocess.run([str(build_tools / 'apksigner'), 'verify', '--verbose', str(apk)], check=True)
    badging = subprocess.check_output([str(build_tools / 'aapt'), 'dump', 'badging', str(apk)], text=True)
    assert "name='ai.tnn.localai'" in badging, 'wrong application ID'
    assert "sdkVersion:'28'" in badging, 'wrong minimum Android version'
    assert "native-code: 'arm64-v8a'" in badging, 'APK must contain only ARM64 code'
    manifest = subprocess.check_output([str(build_tools / 'aapt'), 'dump', 'xmltree', str(apk), 'AndroidManifest.xml'], text=True)
    assert re.search(r'android:extractNativeLibs[^\n]*0xffffffff', manifest), 'engine must be extracted to executable nativeLibraryDir'
    assert re.search(r'android:allowBackup[^\n]*0x0\b', manifest), 'private data backup must remain disabled'
    permissions = subprocess.check_output([str(build_tools / 'aapt'), 'dump', 'permissions', str(apk)], text=True)
    for permission in ('MANAGE_EXTERNAL_STORAGE', 'READ_EXTERNAL_STORAGE', 'WRITE_EXTERNAL_STORAGE', 'RECORD_AUDIO', 'CAMERA'):
        assert f'android.permission.{permission}' not in permissions, f'unexpected permission: {permission}'
    subprocess.run([str(build_tools / 'zipalign'), '-c', '-P', '16', '4', str(apk)], check=True)
    with zipfile.ZipFile(apk) as archive:
        names = archive.namelist()
        config = json.loads(archive.read('assets/tauri.conf.json'))
        assert config['app']['windows'][0]['useHttpsScheme'] is True, 'Android frontend must use the packaged HTTPS protocol'
        assert not any(name.endswith(('.gguf', '.part', '.key', '.jks', '.keystore')) for name in names), 'unexpected model, partial file or signing key'
        app_library = archive.read('lib/arm64-v8a/libtnn_local_ai_lib.so')
        # Tauri embeds frontend data in the Rust shared library, rather than
        # requiring Android asset-manager HTML files or a development server.
        assert b'index.html' in app_library and b'/assets/index-' in app_library, 'embedded frontend asset keys missing'
        for name in ('lib/arm64-v8a/libtnn_local_ai_lib.so', 'lib/arm64-v8a/libtnn_engine.so'):
            data = archive.read(name)
            assert data[:6] == b'\x7fELF\x02\x01', f'{name}: not little-endian ELF64'
            assert struct.unpack_from('<H', data, 18)[0] == 183, f'{name}: not AArch64'
            if name.endswith('libtnn_engine.so'):
                assert struct.unpack_from('<Q', data, 24)[0] != 0, 'engine has no executable entry point'
            offset = struct.unpack_from('<Q', data, 32)[0]
            entry_size, count = struct.unpack_from('<HH', data, 54)
            for index in range(count):
                entry = offset + index * entry_size
                if struct.unpack_from('<I', data, entry)[0] == 1:  # PT_LOAD
                    assert struct.unpack_from('<Q', data, entry + 48)[0] >= 16384, f'{name}: not 16 KB aligned'
        assert any(name.endswith('llama.cpp-LICENSE.txt') for name in names), 'engine licence missing'
    print(f'Verified signed ARM64 APK, engine, assets, licence and ELF page alignment: {apk}')


if __name__ == '__main__':
    verify(Path(sys.argv[1]).resolve())
