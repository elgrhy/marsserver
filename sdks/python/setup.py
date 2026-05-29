"""
Apollo SDK — Python client for the Apollo AI Agent Platform v2.2.

Installation:
    pip install apollo-sdk

    # or from source:
    pip install -e .
"""

from setuptools import setup, find_packages

with open("README.md", encoding="utf-8") as f:
    long_description = f.read()

setup(
    name="apollo-sdk",
    version="2.2.0",
    description="Python client SDK for the Apollo AI Agent Platform",
    long_description=long_description,
    long_description_content_type="text/markdown",
    author="Apollo Platform Contributors",
    author_email="sdk@apollo-platform.io",
    url="https://github.com/elgrhy/apollo",
    license="MIT",
    packages=find_packages(),
    python_requires=">=3.9",
    install_requires=[
        "httpx>=0.27.0",
        "pydantic>=2.0.0",
    ],
    extras_require={
        "dev": [
            "pytest>=8.0",
            "pytest-asyncio>=0.23",
            "respx>=0.21",
        ]
    },
    classifiers=[
        "Development Status :: 5 - Production/Stable",
        "Intended Audience :: Developers",
        "License :: OSI Approved :: MIT License",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
        "Topic :: Software Development :: Libraries :: Python Modules",
        "Topic :: System :: Distributed Computing",
    ],
    keywords="apollo agent ai llm sdk platform",
)
