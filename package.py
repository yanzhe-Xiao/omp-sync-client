import os
import tarfile
import zipfile
import shutil
import io
import time

def make_tarfile(output_filename, source_dir):
    with tarfile.open(output_filename, "w:gz") as tar:
        for root, dirs, files in os.walk(source_dir):
            for file in files:
                full_path = os.path.join(root, file)
                rel_path = os.path.relpath(full_path, source_dir)
                tar.add(full_path, arcname=rel_path)

def create_deb_package(bin_path, deb_out_path, version="1.0.0"):
    # .deb is an ar archive containing:
    # 1. debian-binary (contains "2.0\n")
    # 2. control.tar.gz
    # 3. data.tar.gz

    # 1. debian-binary
    deb_binary = b"2.0\n"

    # 2. control.tar.gz
    control_content = f"""Package: omp-sync-client
Version: {version}
Section: utils
Priority: optional
Architecture: amd64
Maintainer: yanzhe-Xiao <xyzxyzxyz6688@gmail.com>
Description: OMP Multi-Server Config and Skill Bi-directional Sync Client
 High-performance cross-platform sync tool for OMP (Open Multi-Processing) clusters.
 Dual GUI and CLI support with automated Windows/Linux platform path isolation.
""".encode("utf-8")

    control_io = io.BytesIO()
    with tarfile.open(fileobj=control_io, mode="w:gz") as tar:
        ti = tarfile.TarInfo(name="./control")
        ti.size = len(control_content)
        ti.mtime = int(time.time())
        ti.mode = 0o644
        tar.addfile(ti, io.BytesIO(control_content))
    control_tar_gz = control_io.getvalue()

    # 3. data.tar.gz
    data_io = io.BytesIO()
    with tarfile.open(fileobj=data_io, mode="w:gz") as tar:
        # /usr/bin/omp-sync-client
        with open(bin_path, "rb") as f:
            bin_data = f.read()
        ti = tarfile.TarInfo(name="./usr/bin/omp-sync-client")
        ti.size = len(bin_data)
        ti.mtime = int(time.time())
        ti.mode = 0o755
        tar.addfile(ti, io.BytesIO(bin_data))

        # Desktop entry
        desktop_content = """[Desktop Entry]
Name=OMP Sync Client
Comment=OMP Configuration and Skill Syncer
Exec=/usr/bin/omp-sync-client
Icon=system-software-update
Terminal=false
Type=Application
Categories=Utility;Development;
""".encode("utf-8")
        ti_desk = tarfile.TarInfo(name="./usr/share/applications/omp-sync-client.desktop")
        ti_desk.size = len(desktop_content)
        ti_desk.mtime = int(time.time())
        ti_desk.mode = 0o644
        tar.addfile(ti_desk, io.BytesIO(desktop_content))

    data_tar_gz = data_io.getvalue()

    # Format ar archive
    def ar_header(name, size):
        name_field = name.ljust(16)
        mtime_field = str(int(time.time())).ljust(12)
        owner_field = "0".ljust(6)
        group_field = "0".ljust(6)
        mode_field = "100644".ljust(8)
        size_field = str(size).ljust(10)
        trailer = "`\n"
        return (name_field + mtime_field + owner_field + group_field + mode_field + size_field + trailer).encode("ascii")

    with open(deb_out_path, "wb") as deb:
        deb.write(b"!<arch>\n")
        
        # 1. debian-binary
        deb.write(ar_header("debian-binary", len(deb_binary)))
        deb.write(deb_binary)
        if len(deb_binary) % 2 != 0:
            deb.write(b"\n")
            
        # 2. control.tar.gz
        deb.write(ar_header("control.tar.gz", len(control_tar_gz)))
        deb.write(control_tar_gz)
        if len(control_tar_gz) % 2 != 0:
            deb.write(b"\n")
            
        # 3. data.tar.gz
        deb.write(ar_header("data.tar.gz", len(data_tar_gz)))
        deb.write(data_tar_gz)
        if len(data_tar_gz) % 2 != 0:
            deb.write(b"\n")

    print(f"[Package] Created Debian package: {deb_out_path} ({os.path.getsize(deb_out_path)} bytes)")

def package_all():
    os.makedirs("dist", exist_ok=True)
    win_bin = "target/x86_64-pc-windows-gnullvm/release/omp-sync-client.exe"
    linux_bin = "target/x86_64-unknown-linux-musl/release/omp-sync-client"

    # 1. Windows Zip
    win_dist_dir = "dist/omp-sync-client-windows-x64"
    if os.path.exists(win_dist_dir):
        shutil.rmtree(win_dist_dir)
    os.makedirs(win_dist_dir, exist_ok=True)
    shutil.copy(win_bin, os.path.join(win_dist_dir, "omp-sync-client.exe"))
    with open(os.path.join(win_dist_dir, "run.bat"), "w", encoding="utf-8") as f:
        f.write('@echo off\ncd /d "%~dp0"\nstart "" "omp-sync-client.exe"\n')
    shutil.copy("README.md", os.path.join(win_dist_dir, "README.md"))

    win_zip_path = "dist/omp-sync-client-windows-x64.zip"
    with zipfile.ZipFile(win_zip_path, "w", zipfile.ZIP_DEFLATED) as z:
        for root, dirs, files in os.walk(win_dist_dir):
            for file in files:
                p = os.path.join(root, file)
                arcname = os.path.relpath(p, "dist")
                z.write(p, arcname)
    print(f"[Package] Created Windows zip: {win_zip_path} ({os.path.getsize(win_zip_path)} bytes)")

    # 2. Linux Tarball
    linux_dist_dir = "dist/omp-sync-client-linux-x64"
    if os.path.exists(linux_dist_dir):
        shutil.rmtree(linux_dist_dir)
    os.makedirs(linux_dist_dir, exist_ok=True)
    shutil.copy(linux_bin, os.path.join(linux_dist_dir, "omp-sync-client"))
    shutil.copy("README.md", os.path.join(linux_dist_dir, "README.md"))
    with open(os.path.join(linux_dist_dir, "install.sh"), "w", encoding="utf-8", newline="\n") as f:
        f.write("""#!/bin/sh
set -e
echo "Installing omp-sync-client to /usr/local/bin..."
sudo cp omp-sync-client /usr/local/bin/omp-sync-client
sudo chmod +x /usr/local/bin/omp-sync-client
echo "Installation complete! Run 'omp-sync-client' or 'omp-sync-client --help'."
""")

    linux_tar_path = "dist/omp-sync-client-linux-x64.tar.gz"
    with tarfile.open(linux_tar_path, "w:gz") as tar:
        for root, dirs, files in os.walk(linux_dist_dir):
            for file in files:
                p = os.path.join(root, file)
                rel = os.path.relpath(p, "dist")
                ti = tar.gettarinfo(p, arcname=rel)
                if file in ("omp-sync-client", "install.sh"):
                    ti.mode = 0o755
                tar.addfile(ti, open(p, "rb"))
    print(f"[Package] Created Linux tar.gz: {linux_tar_path} ({os.path.getsize(linux_tar_path)} bytes)")

    # 3. Ubuntu / Debian package (.deb)
    deb_path = "dist/omp-sync-client_1.0.0_amd64.deb"
    create_deb_package(linux_bin, deb_path)

if __name__ == "__main__":
    package_all()
