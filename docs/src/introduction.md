# Introduction

Welcome to **dtup**.
A **devicetree toolchain** for better developer experience when developing devicetrees in an embedded environment.

dtup is an open-source and free-software project.
It's repository is hosted on GitHub under this URL: <https://github.com/omekina/dtup>

In this book we will take a look at how to get started with dtup and how to run it's tools on your project.
As the project is written in Rust&mdash;it has extended cross-platform compatibility and you should be fine whether running it on Linux, MacOS or even Windows.
It's also somewhat Android-compatible, but that's not officially supported.

## The goals of dtup

dtup aims to be easy-to-use and friendly to newcomers to devicetree development.
It over-explains everything instead of just telling you something is wrong somewhere near some line.
This is the primary goal&mdash;to achieve ideal error verbosity so the developer using this toolchain can *instantly* tell what's wrong.

## Current scope

Currently, dtup is a validator and a visualizer.
It can be used to view or check a devicetree before submitting a compilation job and building the whole system with a potentially broken devicetree.
