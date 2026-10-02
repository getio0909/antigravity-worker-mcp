#!/usr/bin/env node
import { launch } from '../lib/launcher.mjs';

process.exitCode = await launch(process.argv.slice(2));
