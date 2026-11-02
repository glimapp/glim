#include <errno.h>
#include <inttypes.h>
#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

/* Public libarchive ABI, linked to macOS's /usr/lib/libarchive. */
struct archive;
struct archive_entry;
extern struct archive *archive_read_new(void);
extern int archive_read_support_filter_all(struct archive *);
extern int archive_read_support_format_all(struct archive *);
extern int archive_read_support_format_raw(struct archive *);
extern int archive_read_open_filename(struct archive *, const char *, size_t);
extern int archive_read_next_header(struct archive *, struct archive_entry **);
extern ssize_t archive_read_data(struct archive *, void *, size_t);
extern int archive_read_data_skip(struct archive *);
extern int archive_read_extract(struct archive *, struct archive_entry *, int);
extern int archive_read_free(struct archive *);
extern const char *archive_error_string(struct archive *);
extern int archive_format(struct archive *);
extern const char *archive_entry_pathname_utf8(struct archive_entry *);
extern const char *archive_entry_symlink_utf8(struct archive_entry *);
extern const char *archive_entry_hardlink_utf8(struct archive_entry *);
extern int64_t archive_entry_size(struct archive_entry *);
extern mode_t archive_entry_filetype(struct archive_entry *);
extern mode_t archive_entry_perm(struct archive_entry *);
extern void archive_entry_set_perm(struct archive_entry *, mode_t);
extern void archive_entry_set_pathname(struct archive_entry *, const char *);
extern int archive_entry_is_encrypted(struct archive_entry *);

static int fail(struct archive *reader, const char *message) {
    const char *detail = archive_error_string(reader);
    fprintf(stderr, "%s%s%s\n", message, detail ? ": " : "", detail ? detail : "");
    archive_read_free(reader);
    return 1;
}

static void json_string(const char *value) {
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)value; *p; ++p) {
        if (*p == '"' || *p == '\\') { putchar('\\'); putchar(*p); }
        else if (*p < 0x20) printf("\\u%04x", *p);
        else putchar(*p);
    }
    putchar('"');
}

/* A symlink may use .. only while its resolution remains inside the stage. */
static int safe_path(const char *path, size_t depth) {
    if (!path || !path[0] || path[0] == '/' || strchr(path, '\\')) return 0;
    const char *part = path;
    while (*part) {
        const char *end = strchr(part, '/');
        size_t count = end ? (size_t)(end - part) : strlen(part);
        if (count == 2 && part[0] == '.' && part[1] == '.') {
            if (!depth) return 0;
            --depth;
        } else if (count && !(count == 1 && part[0] == '.')) ++depth;
        if (!end) break;
        part = end + 1;
    }
    return 1;
}

static size_t path_depth(const char *path) {
    size_t depth = 0;
    const char *part = path;
    while (*part) {
        const char *end = strchr(part, '/');
        size_t count = end ? (size_t)(end - part) : strlen(part);
        if (count && !(count == 1 && part[0] == '.')) ++depth;
        if (!end) break;
        part = end + 1;
    }
    return depth;
}

int main(int argc, char **argv) {
    if (argc != 7) return 2;
    setlocale(LC_ALL, "en_US.UTF-8");
    int extract = strcmp(argv[1], "extract") == 0;
    size_t buffer_size = (size_t)strtoull(argv[5], NULL, 10);
    if (!buffer_size) return 2;
    struct archive *reader = archive_read_new();
    if (!reader) return 1;
    archive_read_support_filter_all(reader);
    archive_read_support_format_all(reader);
    if (strcmp(argv[6], "1") == 0) archive_read_support_format_raw(reader);
    if (archive_read_open_filename(reader, argv[2], buffer_size) != 0)
        return fail(reader, "Cannot open archive");
    if (extract && chdir(argv[3]) != 0) return fail(reader, strerror(errno));
    struct archive_entry *entry = NULL;
    int status;
    while ((status = archive_read_next_header(reader, &entry)) == 0) {
        int format = archive_format(reader) & 0xff0000;
        /* mtree descriptions can refer to files outside the archive. */
        if (format == 0x80000) return fail(reader, "mtree is not an archive");
        const char *name = format == 0x90000 ? argv[4] : archive_entry_pathname_utf8(entry);
        if (!name) return fail(reader, "Cannot decode archive filename");
        mode_t type = archive_entry_filetype(entry);
        if (archive_entry_is_encrypted(entry)) return fail(reader, "Encrypted archives require a password");
        if (extract) {
            if (!safe_path(name, 0)) return fail(reader, "Unsafe archive path");
            const char *link = archive_entry_symlink_utf8(entry);
            const char *hardlink = archive_entry_hardlink_utf8(entry);
            if (type != S_IFREG && type != S_IFDIR && type != S_IFLNK && !(type == 0 && hardlink))
                return fail(reader, "Unsupported archive entry type");
            size_t count = path_depth(name);
            size_t depth = count ? count - 1 : 0;
            if ((link && !safe_path(link, depth)) || (hardlink && !safe_path(hardlink, 0)))
                return fail(reader, "Archive link escapes destination");
            if (format == 0x90000) archive_entry_set_pathname(entry, name);
            archive_entry_set_perm(entry, archive_entry_perm(entry) & 0777);
            /* PERM, NO_OVERWRITE, SECURE_SYMLINKS, SECURE_NODOTDOT,
               SECURE_NOABSOLUTEPATHS are stable public libarchive flags. */
            if (archive_read_extract(reader, entry, 0x2 | 0x8 | 0x100 | 0x200 | 0x10000) != 0)
                return fail(reader, "Cannot extract archive entry");
        } else {
            int64_t size = archive_entry_size(entry);
            if (format == 0x90000) {
                char *buffer = malloc(buffer_size);
                if (!buffer) return fail(reader, "Cannot allocate archive buffer");
                size = 0;
                ssize_t count;
                while ((count = archive_read_data(reader, buffer, buffer_size)) > 0) {
                    if (size > INT64_MAX - count) { free(buffer); return fail(reader, "Archive size overflow"); }
                    size += count;
                }
                free(buffer);
                if (count < 0) return fail(reader, "Cannot decode compressed file");
            } else if (archive_read_data_skip(reader) != 0) {
                return fail(reader, "Cannot read archive entry");
            }
            fputs("{\"name\":", stdout); json_string(name);
            printf(",\"size\":%" PRId64 ",\"directory\":%s}\n", size < 0 ? 0 : size,
                   type == S_IFDIR ? "true" : "false");
            if (ferror(stdout)) return fail(reader, "Cannot write archive index");
        }
    }
    if (status != 1) return fail(reader, "Cannot read archive header");
    if (fflush(stdout) != 0) return fail(reader, "Cannot write archive index");
    return archive_read_free(reader) == 0 ? 0 : 1;
}
