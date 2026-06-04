# Build stage
FROM node:20-alpine AS builder

WORKDIR /app

# Copy package files
COPY package.json package-lock.json ./

# Install dependencies
RUN npm ci

# Copy source
COPY . .

# Build
RUN npm run build

# Runtime stage
FROM nginx:alpine

# Copy built assets
COPY --from=builder /app/dist /usr/share/nginx/html

# SPA history fallback + /api/v1 reverse proxy to the in-cluster gateway
COPY nginx.conf /etc/nginx/conf.d/default.conf

# Expose port 80
EXPOSE 80

# Serves /usr/share/nginx/html and proxies /api/v1 (see nginx.conf)
CMD ["nginx", "-g", "daemon off;"]
