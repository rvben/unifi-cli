"""
unifi-cli: CLI for UniFi Network controller.
"""

try:
    from importlib.metadata import version
    __version__ = version("unifi-cli")
except ImportError:
    from importlib_metadata import version
    __version__ = version("unifi-cli")
