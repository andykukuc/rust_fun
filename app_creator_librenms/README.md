app_creator_librenms
====================

Overview
--------
This Rust program parses Speedtest output and updates LibreNMS RRD files for network monitoring. It dynamically detects the system hostname and the appropriate LibreNMS application directory, ensuring compatibility across different environments.

Features
--------
- Parses download and upload speeds from Speedtest output.
- Automatically detects the system hostname.
- Scans for the correct LibreNMS app-speedtest-* directory.
- Updates RRD files using rrdtool.

Usage
-----
1. Ensure Speedtest output is available at:
   /opt/librenms/scripts/speedtest_output.txt

2. The program will automatically detect the hostname and locate the correct app-speedtest-* directory under:
   /data/rrd/<hostname>/

3. Run the program:
   cargo run

Requirements
------------
- Rust toolchain
- rrdtool installed and available in PATH

Notes
-----
- The application directory is detected dynamically; no hardcoded values are required.
- The program prints parsed metrics and error messages for troubleshooting.

License
-------
MIT License

Author
------
Andy Kukuc

Contributor
-----------
GitHub Copilot