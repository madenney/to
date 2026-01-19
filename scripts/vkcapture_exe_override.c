#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static ssize_t (*real_readlink_fn)(const char *, char *, size_t) = NULL;
static ssize_t (*real_readlinkat_fn)(int, const char *, char *, size_t) = NULL;

static ssize_t call_real_readlink(const char *path, char *buf, size_t bufsiz)
{
    if (!real_readlink_fn) {
        real_readlink_fn = (ssize_t (*)(const char *, char *, size_t))dlsym(RTLD_NEXT, "readlink");
    }
    if (!real_readlink_fn) {
        errno = ENOSYS;
        return -1;
    }
    return real_readlink_fn(path, buf, bufsiz);
}

static ssize_t call_real_readlinkat(int dirfd, const char *path, char *buf, size_t bufsiz)
{
    if (!real_readlinkat_fn) {
        real_readlinkat_fn = (ssize_t (*)(int, const char *, char *, size_t))dlsym(RTLD_NEXT, "readlinkat");
    }
    if (!real_readlinkat_fn) {
        errno = ENOSYS;
        return -1;
    }
    return real_readlinkat_fn(dirfd, path, buf, bufsiz);
}

static ssize_t build_override(const char *real_path, const char *override, char *buf, size_t bufsiz)
{
    char out[PATH_MAX];
    const char *slash = strrchr(real_path, '/');
    if (slash) {
        size_t dir_len = (size_t)(slash - real_path);
        if (dir_len == 0) {
            snprintf(out, sizeof(out), "/%s", override);
        } else {
            snprintf(out, sizeof(out), "%.*s/%s", (int)dir_len, real_path, override);
        }
    } else {
        snprintf(out, sizeof(out), "%s", override);
    }

    size_t out_len = strlen(out);
    size_t copy_len = out_len < bufsiz ? out_len : bufsiz;
    if (copy_len > 0 && buf) {
        memcpy(buf, out, copy_len);
    }
    return (ssize_t)copy_len;
}

ssize_t readlink(const char *path, char *buf, size_t bufsiz)
{
    if (!path) {
        errno = EINVAL;
        return -1;
    }

    const char *override = getenv("OBS_VKCAPTURE_EXE_NAME");
    if (!override || !*override || strcmp(path, "/proc/self/exe") != 0) {
        return call_real_readlink(path, buf, bufsiz);
    }

    char real_path[PATH_MAX];
    ssize_t n = call_real_readlink(path, real_path, sizeof(real_path) - 1);
    if (n < 0) {
        return n;
    }
    real_path[n] = '\0';

    return build_override(real_path, override, buf, bufsiz);
}

ssize_t readlinkat(int dirfd, const char *path, char *buf, size_t bufsiz)
{
    if (!path) {
        errno = EINVAL;
        return -1;
    }

    const char *override = getenv("OBS_VKCAPTURE_EXE_NAME");
    if (!override || !*override || strcmp(path, "/proc/self/exe") != 0) {
        return call_real_readlinkat(dirfd, path, buf, bufsiz);
    }

    char real_path[PATH_MAX];
    ssize_t n = call_real_readlinkat(dirfd, path, real_path, sizeof(real_path) - 1);
    if (n < 0) {
        return n;
    }
    real_path[n] = '\0';

    return build_override(real_path, override, buf, bufsiz);
}
