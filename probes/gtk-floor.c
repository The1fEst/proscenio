#include <gtk/gtk.h>
#include <gtk4-layer-shell.h>

static void open_floor(GtkApplication *app, gpointer data) {
    (void)data;
    GListModel *monitors = gdk_display_get_monitors(gdk_display_get_default());
    guint count = g_list_model_get_n_items(monitors);
    for (guint index = 0; index < count; index++) {
        GdkMonitor *monitor = g_list_model_get_item(monitors, index);
        GtkWindow *window = GTK_WINDOW(gtk_application_window_new(app));

        gtk_layer_init_for_window(window);
        gtk_layer_set_namespace(window, "floor:gtk");
        gtk_layer_set_monitor(window, monitor);
        gtk_layer_set_layer(window, GTK_LAYER_SHELL_LAYER_TOP);
        gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_LEFT, TRUE);
        gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_RIGHT, TRUE);
        gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_TOP, TRUE);
        gtk_layer_set_exclusive_zone(window, 0);
        gtk_window_set_default_size(GTK_WINDOW(window), -1, 50);

        gtk_window_present(window);
        g_object_unref(monitor);
    }
}

int main(int argc, char **argv) {
    GtkApplication *app =
        gtk_application_new("org.illogical_impulse.floor.gtk", G_APPLICATION_DEFAULT_FLAGS);
    g_signal_connect(app, "activate", G_CALLBACK(open_floor), NULL);
    int status = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app);
    return status;
}
