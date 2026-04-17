# =============================================================================
# e2e/Dockerfile — Playwright runner with all browsers pre-installed.
#
# Uses Microsoft's official Playwright image so we get Chromium, Firefox,
# WebKit and every system library they need, pinned to a known version.  The
# only thing we add is `npm install` of our test dependencies.
# =============================================================================
FROM mcr.microsoft.com/playwright:v1.47.0-jammy

WORKDIR /e2e

# Install JS deps before copying tests — keeps this layer cached across
# day-to-day spec edits.
COPY package.json package-lock.json* ./
RUN npm install --no-audit --no-fund

COPY . .

# Default: run the headless Playwright suite.  Compose can override this.
ENTRYPOINT ["npx", "playwright", "test"]
