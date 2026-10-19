package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.ArrayAdapter
import android.widget.PopupMenu
import android.widget.Spinner
import android.widget.TextView

/** A native choice control whose displayed value follows its owner's published state. */
internal class MusicSettingsChoice<T>(
    context: Context,
    private val ui: CutoutUi,
    private val label: String,
    private val choices: List<Pair<T, String>>,
    private val changed: (T, () -> Unit) -> Unit,
) : Spinner(context) {
    private var observed: T? = null
    private var allowed = true
    private var pending = false
    private var operation = 0L
    private val main = Handler(Looper.getMainLooper())

    init {
        minimumHeight = ui.dp(48)
        adapter = object : ArrayAdapter<String>(context, android.R.layout.simple_spinner_item, choices.map { it.second }) {
            init { setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item) }
            override fun getView(position: Int, convertView: View?, parent: ViewGroup): View =
                (super.getView(position, convertView, parent) as TextView).apply {
                    setTextColor(ui.foreground)
                    textSize = 17f
                    maxLines = 3
                    setPadding(ui.dp(8), ui.dp(12), ui.dp(8), ui.dp(12))
                }
        }
    }

    /** Shows a native popup and dispatches the explicit tap synchronously, before a later render. */
    override fun performClick(): Boolean {
        if (!isEnabled || choices.isEmpty()) return false
        val selected = observed
        val selectedIndex = choices.indexOfFirst { it.first == selected }
        PopupMenu(context, this).apply {
            choices.forEachIndexed { index, (_, title) ->
                menu.add(0, index, index, title).isCheckable = true
            }
            menu.setGroupCheckable(0, true, true)
            if (selectedIndex >= 0) menu.findItem(selectedIndex)?.isChecked = true
            setOnMenuItemClickListener { item ->
                choose(item.itemId)
                true
            }
            show()
        }
        return true
    }

    private fun choose(index: Int) {
        val choice = choices.getOrNull(index)?.first ?: return
        if (!isEnabled || pending || observed == null || choice == observed) return
        pending = true
        val token = ++operation
        isEnabled = false
        val completed = {
            main.post {
                if (token != operation) return@post
                operation++
                pending = false
                isEnabled = allowed
            }
            Unit
        }
        try {
            changed(choice, completed)
        } catch (_: Exception) {
            completed()
        }
    }

    /** Updates only the displayed selection; it never treats rendering as a user action. */
    fun render(value: T, enabled: Boolean = true) {
        observed = value
        allowed = enabled
        val index = choices.indexOfFirst { it.first == value }
        if (index >= 0 && selectedItemPosition != index) setSelection(index)
        isEnabled = enabled && !pending
        alpha = if (enabled) 1f else 0.55f
        contentDescription = "$label, ${choices.getOrNull(index)?.second.orEmpty()}"
    }
}
